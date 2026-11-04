//! CPU-only GGUF continuation scoring; no generated text or chat parsing.
use core_engine::assistance::*;
use llama_cpp_2::{
    context::{params::LlamaContextParams, LlamaContext},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, LlamaModel},
    token::LlamaToken,
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
        self.cell
            .with_dependent_mut(|_, context| context.clear_kv_cache());
    }
    fn score(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        let result = self.score_inner(request, control);
        if result.is_err() {
            self.reset();
        }
        result
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
