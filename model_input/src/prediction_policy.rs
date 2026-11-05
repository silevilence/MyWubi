//! Model-bound, data-only prediction admission. Thresholds are raw adapter scores,
//! never portable probabilities. A supplied policy is not a quality endorsement.
use core_engine::assistance::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictionPolicy {
    pub version: u32,
    pub adapter: String,
    pub model_sha256: String,
    pub vocabulary_sha256: Option<String>,
    pub minimum_score: f32,
    pub minimum_margin: f32,
    /// Names the calibration dataset, split, procedure and limitations.
    pub evidence: String,
}

impl PredictionPolicy {
    pub fn validates(
        &self,
        adapter: &str,
        model_sha256: &str,
        vocabulary_sha256: Option<&str>,
    ) -> bool {
        self.version == 1
            && self.adapter == adapter
            && self.model_sha256.eq_ignore_ascii_case(model_sha256)
            && match (self.vocabulary_sha256.as_deref(), vocabulary_sha256) {
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                (None, None) => true,
                _ => false,
            }
            && self.minimum_score.is_finite()
            && self.minimum_margin.is_finite()
            && self.minimum_margin >= 0.0
            && !self.evidence.trim().is_empty()
            && self.evidence.len() <= 8192
    }

    fn admit(&self, response: &mut ScoreResponse) -> Result<(), Unavailable> {
        let mut scores: Vec<_> = response.scores.iter().collect();
        scores.sort_by(|a, b| b.value.total_cmp(&a.value));
        let top = scores.first().ok_or(Unavailable::Empty)?.value;
        // A single candidate has no runner-up; only its absolute threshold applies.
        if top < self.minimum_score
            || scores.get(1).is_some_and(|next| {
                f64::from(top) - f64::from(next.value) < f64::from(self.minimum_margin)
            })
        {
            return Err(Unavailable::LowConfidence);
        }
        response.prediction_accepted = Some(
            scores
                .into_iter()
                .filter(|s| s.value >= self.minimum_score)
                .map(|s| s.id)
                .collect(),
        );
        Ok(())
    }
}

pub(crate) struct GatedBackend {
    pub backend: Box<dyn ModelBackend>,
    pub policy: Option<PredictionPolicy>,
}
impl ModelBackend for GatedBackend {
    fn capabilities(&self) -> &Capabilities {
        self.backend.capabilities()
    }
    fn reset(&mut self) {
        self.backend.reset();
    }
    fn warmup(&mut self, control: &ComputeControl) -> Result<(), Unavailable> {
        self.backend.warmup(control)
    }
    fn propose(
        &mut self,
        request: &ProposalRequest,
        control: &ComputeControl,
    ) -> Result<Vec<String>, Unavailable> {
        self.backend.propose(request, control)
    }
    fn score(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        control.check()?;
        if request.task == Task::Predict && self.policy.is_none() {
            return Err(Unavailable::Uncalibrated);
        }
        let mut response = self.backend.score(request, control)?;
        response.validate(request, request.key)?;
        if request.task == Task::Predict {
            if let Some(policy) = &self.policy {
                policy.admit(&mut response)?;
            }
        }
        control.check()?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::candidates::CandidateId;
    fn policy() -> PredictionPolicy {
        PredictionPolicy {
            version: 1,
            adapter: "fixture-v1".into(),
            model_sha256: "a".repeat(64),
            vocabulary_sha256: None,
            minimum_score: -2.0,
            minimum_margin: 0.5,
            evidence: "Authored regression fixture; not a validated model".into(),
        }
    }
    fn response(values: &[f32]) -> ScoreResponse {
        let request: ScoreRequest =
            serde_json::from_str(include_str!("../evaluation/request.json")).unwrap();
        ScoreResponse {
            protocol: 1,
            key: request.key,
            prediction_accepted: None,
            scores: values
                .iter()
                .enumerate()
                .map(|(id, value)| CandidateScore {
                    id: CandidateId(id),
                    value: *value,
                })
                .collect(),
        }
    }
    #[test]
    fn low_absolute_score_and_ambiguous_top_are_silently_rejected() {
        for values in [&[-3.0, -4.0][..], &[-1.0, -1.1], &[-1.0, -1.0]] {
            assert_eq!(
                policy().admit(&mut response(values)),
                Err(Unavailable::LowConfidence)
            );
        }
    }
    #[test]
    fn admitted_ids_exclude_weak_candidates_without_changing_scores() {
        let mut result = response(&[-3.0, -1.0, -2.0]);
        let original = result.scores.clone();
        policy().admit(&mut result).unwrap();
        assert_eq!(result.scores, original);
        assert_eq!(
            result.prediction_accepted,
            Some(vec![CandidateId(1), CandidateId(2)])
        );
        assert!(policy().admit(&mut response(&[-1.0])).is_ok());
    }
    #[test]
    fn policy_is_bound_to_model_and_adapter_and_requires_finite_thresholds() {
        let mut p = policy();
        assert!(p.validates("fixture-v1", &"a".repeat(64), None));
        assert!(!p.validates("fixture-v2", &"a".repeat(64), None));
        assert!(!p.validates("fixture-v1", &"b".repeat(64), None));
        assert!(!p.validates("fixture-v1", &"a".repeat(64), Some("changed-vocabulary")));
        p.minimum_margin = -1.;
        assert!(!p.validates("fixture-v1", &"a".repeat(64), None));
        p.minimum_margin = f32::NAN;
        assert!(!p.validates("fixture-v1", &"a".repeat(64), None));
    }

    #[test]
    fn package_gate_rejects_uncalibrated_predict_but_keeps_raw_rank_evaluation() {
        struct Raw {
            caps: Capabilities,
            calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }
        impl ModelBackend for Raw {
            fn capabilities(&self) -> &Capabilities {
                &self.caps
            }
            fn reset(&mut self) {}
            fn score(
                &mut self,
                r: &ScoreRequest,
                _: &ComputeControl,
            ) -> Result<ScoreResponse, Unavailable> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(ScoreResponse {
                    protocol: 1,
                    key: r.key,
                    prediction_accepted: None,
                    scores: r
                        .candidates
                        .iter()
                        .enumerate()
                        .map(|(i, c)| CandidateScore {
                            id: c.id,
                            value: -1.0 - i as f32 * 3.0,
                        })
                        .collect(),
                })
            }
        }
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let raw = Raw {
            calls: calls.clone(),
            caps: Capabilities {
                protocol: 1,
                tasks: vec![Task::Rank, Task::Predict],
                languages: vec!["zh".into()],
                max_context_chars: 256,
                max_candidates: 64,
                max_candidate_chars: 16,
                max_code_chars: 64,
                proposes_words: false,
                incremental: false,
                cooperative_cancel: true,
            },
        };
        let mut backend = GatedBackend {
            backend: Box::new(raw),
            policy: None,
        };
        let control = ComputeControl {
            cancellation: Cancellation::default(),
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(5),
        };
        let mut request: ScoreRequest =
            serde_json::from_str(include_str!("../evaluation/request.json")).unwrap();
        request.task = Task::Predict;
        assert_eq!(
            backend.score(&request, &control),
            Err(Unavailable::Uncalibrated)
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        request.task = Task::Rank;
        assert!(backend
            .score(&request, &control)
            .unwrap()
            .prediction_accepted
            .is_none());
        request.task = Task::Predict;
        backend.policy = Some(policy());
        let admitted = backend.score(&request, &control).unwrap();
        assert_eq!(
            admitted.prediction_accepted,
            Some(vec![request.candidates[0].id])
        );
        backend.policy.as_mut().unwrap().minimum_score = 0.;
        assert_eq!(
            backend.score(&request, &control),
            Err(Unavailable::LowConfidence)
        );
    }
}
