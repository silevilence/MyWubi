//! ONNX Runtime CPU adapter for the documented character-step-v1 graph.
use core_engine::assistance::*;
use ort::{environment::Environment, session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
    sync::{Arc, OnceLock},
};

static ENVIRONMENT: OnceLock<Result<Environment, Unavailable>> = OnceLock::new();

/// Initialize the application-bundled runtime, never a model-supplied native library.
/// Android must pass its nativeLibraryDir/libonnxruntime.so before loading a model.
pub fn initialize_runtime(path: &Path) -> Result<(), Unavailable> {
    if !path.is_absolute() || !path.is_file() {
        return Err(Unavailable::Load);
    }
    ENVIRONMENT
        .get_or_init(|| {
            ort::init_from(path)
                .map_err(|_| Unavailable::Load)?
                .with_logger(Arc::new(|_, _, _, _, _| {}))
                .build()
                .map_err(|_| Unavailable::Load)
        })
        .as_ref()
        .map(|_| ())
        .map_err(|error| *error)
}

/// Fixed vocabulary: index 0 is unknown, index 1 is BOS, other tokens are single scalars.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StepVocabulary {
    pub adapter: String,
    pub tokens: Vec<String>,
    pub hidden_size: usize,
}

#[derive(Clone)]
struct State {
    h: Vec<f32>,
    c: Vec<f32>,
    logits: Vec<f32>,
}

/// One recurrent state owner and CPU session. No custom operators or package scripts.
pub struct OnnxScorer {
    session: Session,
    vocabulary: StepVocabulary,
    ids: BTreeMap<char, usize>,
    capabilities: Capabilities,
    prefix: String,
    cached: Option<State>,
}

impl OnnxScorer {
    pub fn load(model: &Path, vocabulary: &Path, threads: usize) -> Result<Self, Unavailable> {
        if !model.is_file() || !vocabulary.is_file() {
            return Err(Unavailable::MissingModel);
        }
        if !(1..=8).contains(&threads) {
            return Err(Unavailable::Limit);
        }
        if std::fs::metadata(vocabulary)
            .map_err(|_| Unavailable::Load)?
            .len()
            > 1024 * 1024
            || std::fs::metadata(model)
                .map_err(|_| Unavailable::Load)?
                .len()
                > 64 * 1024 * 1024
        {
            return Err(Unavailable::Limit);
        }
        let bytes = std::fs::read(vocabulary).map_err(|_| Unavailable::Load)?;
        if bytes.len() > 1024 * 1024 {
            return Err(Unavailable::Limit);
        }
        let vocabulary: StepVocabulary =
            serde_json::from_slice(&bytes).map_err(|_| Unavailable::UnsupportedModel)?;
        let unique: HashSet<_> = vocabulary.tokens.iter().collect();
        if vocabulary.adapter != "character-step-v1"
            || !(1..=64).contains(&vocabulary.hidden_size)
            || !(3..=10000).contains(&vocabulary.tokens.len())
            || unique.len() != vocabulary.tokens.len()
            || vocabulary.tokens[0] != "<unk>"
            || vocabulary.tokens[1] != "<bos>"
            || vocabulary.tokens[2..]
                .iter()
                .any(|t| t.chars().count() != 1)
        {
            return Err(Unavailable::UnsupportedModel);
        }
        if ENVIRONMENT.get().is_none() {
            let executable = std::env::current_exe().map_err(|_| Unavailable::Load)?;
            let directory = executable.parent().ok_or(Unavailable::Load)?;
            let name = if cfg!(target_os = "windows") {
                "onnxruntime.dll"
            } else {
                "libonnxruntime.so"
            };
            initialize_runtime(&directory.join(name))?;
        }
        let environment = ENVIRONMENT
            .get()
            .ok_or(Unavailable::Load)?
            .as_ref()
            .map_err(|e| *e)?;
        // Memory loading deliberately has no model-directory base for external weights.
        // character-step-v1 packages require embedded tensors; no arbitrary filesystem reads.
        let model_bytes = std::fs::read(model).map_err(|_| Unavailable::Load)?;
        let session = Session::builder(environment)
            .map_err(|_| Unavailable::Load)?
            .with_intra_threads(threads)
            .map_err(|_| Unavailable::Load)?
            .with_inter_threads(1)
            .map_err(|_| Unavailable::Load)?
            .commit_from_memory(&model_bytes)
            .map_err(|_| Unavailable::UnsupportedModel)?;
        let ids = vocabulary
            .tokens
            .iter()
            .enumerate()
            .skip(2)
            .filter_map(|(i, t)| t.chars().next().map(|c| (c, i)))
            .collect();
        let mut scorer = Self {
            session,
            vocabulary,
            ids,
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
            prefix: String::new(),
            cached: None,
        };
        // A real probe rejects incompatible shapes/types/operators before activation.
        scorer.step(1, &scorer.zero())?;
        Ok(scorer)
    }

    fn zero(&self) -> State {
        State {
            h: vec![0.; self.vocabulary.hidden_size],
            c: vec![0.; self.vocabulary.hidden_size],
            logits: vec![0.; self.vocabulary.tokens.len()],
        }
    }

    fn step(&mut self, token: usize, state: &State) -> Result<State, Unavailable> {
        let n = self.vocabulary.tokens.len();
        let h = self.vocabulary.hidden_size;
        let mut one_hot = vec![0f32; n];
        *one_hot.get_mut(token).ok_or(Unavailable::Inference)? = 1.;
        let x = Tensor::from_array((vec![1i64, 1, n as i64], one_hot))
            .map_err(|_| Unavailable::Inference)?;
        let hidden = Tensor::from_array((vec![1i64, 1, h as i64], state.h.clone()))
            .map_err(|_| Unavailable::Inference)?;
        let cell = Tensor::from_array((vec![1i64, 1, h as i64], state.c.clone()))
            .map_err(|_| Unavailable::Inference)?;
        let outputs = self
            .session
            .run(ort::inputs!["x"=>x,"h"=>hidden,"c"=>cell])
            .map_err(|_| Unavailable::Inference)?;
        let extract = |name: &str, shape: &[i64]| -> Result<Vec<f32>, Unavailable> {
            let value = outputs.get(name).ok_or(Unavailable::UnsupportedModel)?;
            let (actual, values) = value
                .try_extract_tensor::<f32>()
                .map_err(|_| Unavailable::UnsupportedModel)?;
            if &actual[..] != shape {
                return Err(Unavailable::UnsupportedModel);
            }
            if values.iter().any(|v| !v.is_finite()) {
                return Err(Unavailable::NonFinite);
            }
            Ok(values.to_vec())
        };
        Ok(State {
            h: extract("next_h", &[1, 1, h as i64])?,
            c: extract("next_c", &[1, 1, h as i64])?,
            logits: extract("logits", &[1, n as i64])?,
        })
    }

    fn score_inner(
        &mut self,
        r: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        r.validate(&self.capabilities)?;
        control.check()?;
        if self.cached.is_none() || self.prefix != r.prefix {
            let mut state = self.step(1, &self.zero())?;
            for ch in r.prefix.chars() {
                control.check()?;
                state = self.step(self.ids.get(&ch).copied().unwrap_or(0), &state)?;
            }
            self.prefix = r.prefix.clone();
            self.cached = Some(state);
        }
        let prefix = self.cached.as_ref().ok_or(Unavailable::Inference)?.clone();
        let mut scores = Vec::new();
        for candidate in &r.candidates {
            let mut state = prefix.clone();
            let mut sum = 0.;
            for ch in candidate.text.chars() {
                control.check()?;
                let token = self.ids.get(&ch).copied().unwrap_or(0);
                let max = state
                    .logits
                    .iter()
                    .copied()
                    .fold(f32::NEG_INFINITY, f32::max);
                let norm = state
                    .logits
                    .iter()
                    .map(|v| f64::from(*v - max).exp())
                    .sum::<f64>()
                    .ln();
                sum += f64::from(state.logits[token] - max) - norm;
                state = self.step(token, &state)?;
            }
            scores.push(CandidateScore {
                id: candidate.id,
                value: (sum / candidate.text.chars().count() as f64) as f32,
            });
        }
        control.check()?;
        Ok(ScoreResponse {
            protocol: 1,
            key: r.key,
            scores,
        })
    }
}

impl ModelBackend for OnnxScorer {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
    fn reset(&mut self) {
        self.prefix.clear();
        self.cached = None;
    }
    fn score(
        &mut self,
        r: &ScoreRequest,
        c: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        let result = self.score_inner(r, c);
        if result.is_err() {
            self.reset();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn fixture(kind: &str) -> OnnxScorer {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("evaluation/onnx");
        OnnxScorer::load(
            &root.join(format!("{kind}.onnx")),
            &root.join("vocabulary.json"),
            1,
        )
        .unwrap()
    }

    #[test]
    fn rust_runtime_matches_numpy_reference_for_stateless_and_lstm_exports() {
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("../evaluation/onnx/numerical-reference.json"))
                .unwrap();
        for kind in ["counts", "lstm"] {
            let mut scorer = fixture(kind);
            for case in reference[kind]["cases"].as_array().unwrap() {
                let floats = |field: &str| {
                    case[field]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap() as f32)
                        .collect::<Vec<_>>()
                };
                let state = State {
                    h: floats("h"),
                    c: floats("c"),
                    logits: vec![],
                };
                let next = scorer
                    .step(case["token"].as_u64().unwrap() as usize, &state)
                    .unwrap();
                for (actual, expected) in [
                    (&next.h, floats("next_h")),
                    (&next.c, floats("next_c")),
                    (&next.logits, floats("logits")),
                ] {
                    assert_eq!(actual.len(), expected.len());
                    assert!(actual
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| (a - b).abs() < 1e-6));
                }
            }
        }
    }

    #[test]
    fn candidate_branches_are_isolated_and_repeated_scores_survive_reset() {
        let mut r: ScoreRequest =
            serde_json::from_str(include_str!("../evaluation/request.json")).unwrap();
        let c = ComputeControl {
            cancellation: Cancellation::default(),
            deadline: Instant::now() + Duration::from_secs(10),
        };
        for kind in ["counts", "lstm"] {
            let mut scorer = fixture(kind);
            let first = scorer.score(&r, &c).unwrap();
            r.candidates.reverse();
            let second = scorer.score(&r, &c).unwrap();
            for value in &first.scores {
                assert_eq!(Some(value), second.scores.iter().find(|s| s.id == value.id));
            }
            scorer.reset();
            assert_eq!(second, scorer.score(&r, &c).unwrap());
        }
    }

    #[test]
    fn unsupported_operators_and_output_shapes_are_rejected_before_activation() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("evaluation/onnx");
        for name in ["bad-shape.onnx", "bad-operator.onnx"] {
            assert!(matches!(
                OnnxScorer::load(&root.join(name), &root.join("vocabulary.json"), 1),
                Err(Unavailable::UnsupportedModel)
            ));
        }
    }
}
