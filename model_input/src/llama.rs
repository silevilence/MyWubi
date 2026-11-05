//! CPU-only GGUF scoring and bounded word proposals.
use core_engine::assistance::*;
use llama_cpp_2::{
    context::{params::LlamaContextParams, LlamaContext},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, LlamaModel},
    sampling::LlamaSampler,
    token::{data::LlamaTokenData, data_array::LlamaTokenDataArray, LlamaToken},
};
use std::{num::NonZeroU32, path::Path, sync::OnceLock};

// Runtime metadata stays process-local; models and contexts are released on drop.
// A process-lifetime initializer avoids racing native init/free during model swaps.
static BACKEND: OnceLock<Result<LlamaBackend, Unavailable>> = OnceLock::new();

struct LoadedModel {
    model: LlamaModel,
    backend: &'static LlamaBackend,
}
self_cell::self_cell! {
    struct ContextCell {
        owner: LoadedModel,
        #[not_covariant]
        dependent: LlamaContext,
    }
}

/// One model/context per worker. Prefix state survives identical-context requests;
/// candidate branches are removed before every score, including the first.
pub struct LlamaScorer {
    cell: ContextCell,
    capabilities: Capabilities,
    cached_prefix: Vec<LlamaToken>,
    prefix_logits: Vec<f32>,
    prefix_log_normalizer: f64,
    context_tokens: usize,
    generation: Option<crate::generation::GenerationPolicy>,
    generation_header: Vec<LlamaToken>,
    generation_preface: Vec<LlamaToken>,
    generation_header_ready: bool,
}

impl LlamaScorer {
    pub fn load(path: &Path, context_tokens: u32, threads: i32) -> Result<Self, Unavailable> {
        if !path.is_file() {
            return Err(Unavailable::MissingModel);
        }
        if !(64..=4096).contains(&context_tokens) || !(1..=8).contains(&threads) {
            return Err(Unavailable::Limit);
        }
        let backend = BACKEND
            .get_or_init(|| {
                let mut backend = LlamaBackend::init().map_err(|_| Unavailable::Load)?;
                backend.void_logs();
                Ok(backend)
            })
            .as_ref()
            .map_err(|error| *error)?;
        let model = LlamaModel::load_from_file(
            backend,
            path,
            &LlamaModelParams::default().with_n_gpu_layers(0),
        )
        .map_err(|_| Unavailable::UnsupportedModel)?;
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(context_tokens))
            .with_n_batch(context_tokens)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let cell = ContextCell::try_new(LoadedModel { model, backend }, |owner| {
            owner.model.new_context(owner.backend, params)
        })
        .map_err(|_| Unavailable::Load)?;
        Ok(Self {
            cell,
            capabilities: Capabilities {
                protocol: 1,
                tasks: vec![Task::Rank, Task::Predict],
                languages: vec!["zh".into()],
                max_context_chars: 256,
                max_candidates: 64,
                max_candidate_chars: 16,
                max_code_chars: 64,
                proposes_words: false,
                incremental: true,
                cooperative_cancel: true,
            },
            cached_prefix: vec![],
            prefix_logits: vec![],
            prefix_log_normalizer: 0.,
            context_tokens: context_tokens as usize,
            generation: None,
            generation_header: vec![],
            generation_preface: vec![],
            generation_header_ready: false,
        })
    }

    /// Enable only a separately validated, model-bound generation package.
    pub fn with_generation(mut self, policy: crate::generation::GenerationPolicy) -> Self {
        let vocab = self.cell.borrow_owner().model.vocab();
        self.generation_header = vocab.tokenize(b"<|im_start|>system\n", true, true);
        self.generation_header.extend(vocab.tokenize("补全前文的下一个词。仅输出新增部分，最多三个候选，JSON数组。例如前文“我想喝”，输出[“水”,“茶”,“咖啡”]。不要重复前文或生成句子。无法续接输出[]。".as_bytes(), false, false));
        self.generation_header.extend(vocab.tokenize(
            b"<|im_end|>\n<|im_start|>user\n",
            false,
            true,
        ));
        self.generation_preface = vocab.tokenize("前文：".as_bytes(), false, false);
        self.generation_header.extend(&self.generation_preface);
        self.capabilities.proposes_words = true;
        self.generation = Some(policy);
        self
    }

    fn prepare_generation(&mut self, control: &ComputeControl) -> Result<(), Unavailable> {
        control.check()?;
        if self.generation_header_ready {
            return Ok(());
        }
        if self.generation_header.is_empty() || self.generation_header.len() >= self.context_tokens
        {
            return Err(Unavailable::Limit);
        }
        self.cached_prefix.clear();
        self.prefix_logits.clear();
        self.cell.with_dependent_mut(|_, context| {
            context.clear_kv_cache();
            let mut batch = LlamaBatch::new(self.generation_header.len(), 1);
            batch
                .add_sequence(&self.generation_header, 0, false)
                .map_err(|_| Unavailable::Inference)?;
            context
                .decode(&mut batch)
                .map_err(|_| Unavailable::Inference)
        })?;
        self.generation_header_ready = true;
        control.check()
    }

    fn generate(
        &mut self,
        request: &ProposalRequest,
        control: &ComputeControl,
    ) -> Result<Vec<String>, Unavailable> {
        request.validate(&self.capabilities)?;
        self.generate_words(&request.prefix, request.limit, control)
    }

    fn generate_words(
        &mut self,
        prefix: &str,
        limit: usize,
        control: &ComputeControl,
    ) -> Result<Vec<String>, Unavailable> {
        control.check()?;
        if prefix.ends_with(['。', '！', '？', '\n']) {
            return Err(Unavailable::Empty);
        }
        self.prepare_generation(control)?;
        let policy = self.generation.as_ref().ok_or(Unavailable::Uncalibrated)?;
        // Encode user text separately with special-token parsing disabled. A
        // literal ChatML marker in the host must not become a role delimiter.
        let model = &self.cell.borrow_owner().model;
        let vocab = model.vocab();
        let user_tokens = vocab.tokenize(format!("前文：{}", prefix).as_bytes(), false, false);
        // Keep exactly the original tokens even if a tokenizer merges the
        // boundary between the fixed preface and arbitrary host text.
        let reusable = user_tokens.starts_with(&self.generation_preface);
        let header_len = self.generation_header.len()
            - if reusable {
                0
            } else {
                self.generation_preface.len()
            };
        let mut tokens = if reusable {
            user_tokens[self.generation_preface.len()..].to_vec()
        } else {
            user_tokens
        };
        tokens.extend(vocab.tokenize(b"<|im_end|>\n<|im_start|>assistant\n", false, true));
        if tokens.is_empty() || header_len + tokens.len() + policy.max_tokens > self.context_tokens
        {
            return Err(Unavailable::Limit);
        }
        let words = policy.max_words.min(limit);
        let grammar = format!("root ::= \"[\" ws (word (ws \",\" ws word){{0,{}}})? ws \"]\"\nword ::= \"\\\"\" [一-鿿]{{1,{}}} \"\\\"\"\nws ::= [ \\t\\n]{{0,4}}", words - 1, policy.max_chars);
        let mut sampler = LlamaSampler::chain(
            [
                LlamaSampler::grammar(model, &grammar, "root")
                    .map_err(|_| Unavailable::UnsupportedModel)?,
                LlamaSampler::greedy(),
            ],
            true,
        );
        if !reusable {
            self.generation_header_ready = false;
        }
        self.cell.with_dependent_mut(|owner, context| {
            if !reusable
                && !context
                    .clear_kv_cache_seq(Some(0), Some(header_len as u32), None)
                    .map_err(|_| Unavailable::Inference)?
            {
                return Err(Unavailable::Inference);
            }
            let mut batch = LlamaBatch::new(tokens.len(), 1);
            for (index, token) in tokens.iter().enumerate() {
                batch
                    .add(
                        *token,
                        (header_len + index) as i32,
                        &[0],
                        index + 1 == tokens.len(),
                    )
                    .map_err(|_| Unavailable::Inference)?;
            }
            context
                .decode(&mut batch)
                .map_err(|_| Unavailable::Inference)?;
            let mut bytes = Vec::new();
            let mut sum = 0.;
            let mut index = (tokens.len() - 1) as i32;
            for step in 0..policy.max_tokens {
                control.check()?;
                let token = constrained_greedy(&mut sampler, context, index)?;
                if owner.model.vocab().is_eog(token) {
                    return Err(Unavailable::Empty);
                }
                sum += log_probability(context.get_logits_ith(index), token)?;
                bytes.extend(owner.model.vocab().token_to_piece(token, false, None));
                if bytes.len() > 1024 {
                    return Err(Unavailable::Limit);
                }
                if bytes.last() == Some(&b']') {
                    control.check()?;
                    return policy.finish(prefix, &bytes, sum / (step + 1) as f32, limit);
                }
                let mut batch = LlamaBatch::new(1, 1);
                batch
                    .add(token, (header_len + tokens.len() + step) as i32, &[0], true)
                    .map_err(|_| Unavailable::Inference)?;
                context
                    .decode(&mut batch)
                    .map_err(|_| Unavailable::Inference)?;
                index = 0;
            }
            Err(Unavailable::Limit)
        })
    }

    fn score_inner(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        request.validate(&self.capabilities)?;
        control.check()?;
        // A newline provides an explicit context for tokenizers with no automatic BOS.
        let prefix = if request.prefix.is_empty() {
            "\n"
        } else {
            &request.prefix
        };
        let vocab = self.cell.borrow_owner().model.vocab();
        let prefix_tokens = vocab.tokenize(prefix.as_bytes(), true, false);
        let candidates: Vec<_> = request
            .candidates
            .iter()
            .map(|c| vocab.tokenize(c.text.as_bytes(), false, false))
            .collect();
        if prefix_tokens.is_empty()
            || candidates
                .iter()
                .any(|c| c.is_empty() || c.len() + prefix_tokens.len() > self.context_tokens)
        {
            return Err(Unavailable::Limit);
        }
        if prefix_tokens != self.cached_prefix {
            self.cell
                .with_dependent_mut(|_, context| -> Result<(), Unavailable> {
                    context.clear_kv_cache();
                    let mut batch = LlamaBatch::new(prefix_tokens.len(), 1);
                    batch
                        .add_sequence(&prefix_tokens, 0, false)
                        .map_err(|_| Unavailable::Inference)?;
                    context
                        .decode(&mut batch)
                        .map_err(|_| Unavailable::Inference)?;
                    self.prefix_logits = context
                        .get_logits_ith((prefix_tokens.len() - 1) as i32)
                        .to_vec();
                    self.prefix_log_normalizer = log_normalizer(&self.prefix_logits)?;
                    Ok(())
                })?;
            self.cached_prefix = prefix_tokens;
        }
        let prefix_len = self.cached_prefix.len();
        let mut scores = Vec::with_capacity(candidates.len());
        for (candidate, tokens) in request.candidates.iter().zip(&candidates) {
            control.check()?;
            let value = self
                .cell
                .with_dependent_mut(|_, context| -> Result<f32, Unavailable> {
                    if !context
                        .clear_kv_cache_seq(Some(0), Some(prefix_len as u32), None)
                        .map_err(|_| Unavailable::Inference)?
                    {
                        return Err(Unavailable::UnsupportedModel);
                    }
                    let target = *self
                        .prefix_logits
                        .get(usize::try_from(tokens[0].0).map_err(|_| Unavailable::Inference)?)
                        .ok_or(Unavailable::Inference)?;
                    let mut sum = (f64::from(target) - self.prefix_log_normalizer) as f32;
                    for i in 1..tokens.len() {
                        control.check()?;
                        let mut batch = LlamaBatch::new(1, 1);
                        batch
                            .add(tokens[i - 1], (prefix_len + i - 1) as i32, &[0], true)
                            .map_err(|_| Unavailable::Inference)?;
                        context
                            .decode(&mut batch)
                            .map_err(|_| Unavailable::Inference)?;
                        sum += log_probability(context.get_logits_ith(0), tokens[i])?;
                    }
                    Ok(sum / tokens.len() as f32)
                })?;
            scores.push(CandidateScore {
                id: candidate.id,
                value,
            });
        }
        control.check()?;
        Ok(ScoreResponse {
            prediction_accepted: None,
            protocol: 1,
            key: request.key,
            scores,
        })
    }
}

impl ModelBackend for LlamaScorer {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    fn reset(&mut self) {
        self.cached_prefix.clear();
        self.prefix_logits.clear();
        // Only the fixed public adapter prompt may survive owner changes. All
        // host text and generated tokens start after this boundary and are removed.
        let header_len = self.generation_header.len() as u32;
        let keep_header = self.generation_header_ready;
        self.generation_header_ready = self.cell.with_dependent_mut(|_, context| {
            if keep_header
                && context
                    .clear_kv_cache_seq(Some(0), Some(header_len), None)
                    .unwrap_or(false)
            {
                true
            } else {
                context.clear_kv_cache();
                false
            }
        });
    }
    fn warmup(&mut self, control: &ComputeControl) -> Result<(), Unavailable> {
        if self.generation.is_some() {
            // Exercise the actual grammar, output projection and both decode
            // shapes using fixed public text. Never preload a host's context.
            let result = self.generate_words("预热完成", 1, control);
            self.reset();
            match result {
                Ok(_) | Err(Unavailable::Empty | Unavailable::LowConfidence) => control.check(),
                Err(error) => Err(error),
            }
        } else {
            control.check()?;
            let tokens =
                self.cell
                    .borrow_owner()
                    .model
                    .vocab()
                    .tokenize("预热完成".as_bytes(), true, false);
            let result = self.cell.with_dependent_mut(|_, context| {
                context.clear_kv_cache();
                let mut batch = LlamaBatch::new(tokens.len(), 1);
                batch
                    .add_sequence(&tokens, 0, false)
                    .map_err(|_| Unavailable::Inference)?;
                context
                    .decode(&mut batch)
                    .map_err(|_| Unavailable::Inference)?;
                control.check()
            });
            self.reset();
            result
        }
    }
    fn propose(
        &mut self,
        request: &ProposalRequest,
        control: &ComputeControl,
    ) -> Result<Vec<String>, Unavailable> {
        let result = self.generate(request, control);
        self.reset();
        result
    }
    fn score(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        // Scoring uses its own prefix cache and replaces any public prompt KV.
        self.generation_header_ready = false;
        let result = self.score_inner(request, control);
        if result.is_err() {
            self.reset();
        }
        result
    }
}

/// Match grammar-first greedy exactly, but test the global winner first.
/// Most model tokens already obey the grammar; scanning the whole vocabulary
/// through the grammar on those steps needlessly delays interactive decoding.
fn constrained_greedy(
    sampler: &mut LlamaSampler,
    context: &LlamaContext<'_>,
    index: i32,
) -> Result<LlamaToken, Unavailable> {
    let logits = context.get_logits_ith(index);
    let mut best = 0;
    for (i, value) in logits.iter().enumerate() {
        if !value.is_finite() {
            return Err(Unavailable::NonFinite);
        }
        if *value > logits[best] {
            best = i;
        }
    }
    let value = *logits.get(best).ok_or(Unavailable::Inference)?;
    let token = LlamaToken(i32::try_from(best).map_err(|_| Unavailable::Limit)?);
    let mut candidate =
        LlamaTokenDataArray::new(vec![LlamaTokenData::new(token, value, 0.)], false);
    sampler.apply(&mut candidate);
    if candidate.data[0].logit().is_finite() {
        sampler.accept(token);
        Ok(token)
    } else {
        // A grammar-valid token in the highest-ranked subset is necessarily
        // the full-vocabulary greedy winner. Expand only when every member is
        // rejected. Original vocabulary order preserves greedy tie breaking.
        let mut ranked: Vec<usize> = (0..logits.len()).collect();
        for count in [32, 256, 2048] {
            if count >= ranked.len() {
                break;
            }
            ranked.select_nth_unstable_by(count, |a, b| {
                if logits[*a] == logits[*b] {
                    a.cmp(b)
                } else {
                    logits[*b].total_cmp(&logits[*a])
                }
            });
            let mut subset = ranked[..count].to_vec();
            subset.sort_unstable();
            let mut candidates = LlamaTokenDataArray::new(
                subset
                    .into_iter()
                    .map(|i| LlamaTokenData::new(LlamaToken(i as i32), logits[i], 0.))
                    .collect(),
                false,
            );
            sampler.apply(&mut candidates);
            if let Some(selected) = candidates.selected {
                if candidates.data[selected].logit().is_finite() {
                    let token = candidates.data[selected].id();
                    sampler.accept(token);
                    return Ok(token);
                }
            }
        }
        Ok(sampler.sample(context, index))
    }
}

fn log_probability(logits: &[f32], token: LlamaToken) -> Result<f32, Unavailable> {
    let target = *logits
        .get(usize::try_from(token.0).map_err(|_| Unavailable::Inference)?)
        .ok_or(Unavailable::Inference)?;
    let value = (f64::from(target) - log_normalizer(logits)?) as f32;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Unavailable::NonFinite)
    }
}

fn log_normalizer(logits: &[f32]) -> Result<f64, Unavailable> {
    if logits.is_empty() || logits.iter().any(|v| !v.is_finite()) {
        return Err(Unavailable::NonFinite);
    }
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let log_sum = logits
        .iter()
        .map(|v| f64::from(*v - max).exp())
        .sum::<f64>()
        .ln();
    let value = f64::from(max) + log_sum;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Unavailable::NonFinite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires MYWUBI_TEST_GGUF; compares optimized and reference grammar sampling"]
    fn constrained_greedy_matches_full_vocabulary_grammar_sampling() {
        let path = std::env::var("MYWUBI_TEST_GGUF").unwrap();
        let mut backend = LlamaScorer::load(Path::new(&path), 1024, 2).unwrap();
        let grammar = "root ::= \"[\" (\"\\\"\" [一-鿿]{1,6} \"\\\"\")? \"]\"";
        for prefix in ["北京", "明天去", "研究量子", "结束。"] {
            backend.cell.with_dependent_mut(|owner,context| {
                context.clear_kv_cache();
                let tokens=owner.model.vocab().tokenize(format!("<|im_start|>user\n补全前文的下一个词，仅输出JSON数组。前文：{prefix}<|im_end|>\n<|im_start|>assistant\n").as_bytes(),true,true);
                let make_sampler=|| LlamaSampler::chain([LlamaSampler::grammar(&owner.model,grammar,"root").unwrap(),LlamaSampler::greedy()],true);
                let mut reference=make_sampler();let mut fast=make_sampler();
                let mut batch=LlamaBatch::new(tokens.len(),1);
                batch.add_sequence(&tokens,0,true).unwrap();context.decode(&mut batch).unwrap();
                let mut index=(tokens.len()-1) as i32;
                for step in 0..16 {
                    let expected=reference.sample(context,index);
                    let actual=constrained_greedy(&mut fast,context,index).unwrap();
                    assert_eq!(actual,expected,"prefix {prefix}, step {step}");
                    if owner.model.vocab().is_eog(actual) { break; }
                    let mut batch=LlamaBatch::new(1,1);
                    batch.add(actual,(tokens.len()+step) as i32,&[0],true).unwrap();context.decode(&mut batch).unwrap();
                    index=0;
                }
            });
        }
    }
    #[test]
    fn log_softmax_is_stable_and_rejects_invalid_logits() {
        assert!(
            (log_probability(&[1000., 1000.], LlamaToken(0)).unwrap() + 2f32.ln()).abs() < 1e-6
        );
        assert_eq!(
            log_probability(&[f32::NAN], LlamaToken(0)),
            Err(Unavailable::NonFinite)
        );
    }
}
