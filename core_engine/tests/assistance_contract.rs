use core_engine::{
    assistance::*,
    candidates::{CandidateId, QueryRevision},
    session::EditToken,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn request() -> ScoreRequest {
    ScoreRequest {
        protocol: 1,
        key: RequestKey {
            edit: EditToken {
                session: 1,
                generation: 2,
                editor: 3,
                epoch: 4,
                runtime: 5,
            },
            query: QueryRevision {
                engine: 6,
                version: 7,
            },
            view: 8,
            context: 9,
            model: 10,
            config: 11,
            request: 12,
        },
        task: Task::Rank,
        language: "zh".into(),
        prefix: "下周去".into(),
        code: "xian".into(),
        candidates: vec![
            ScoringCandidate {
                id: CandidateId(4),
                text: "仙".into(),
            },
            ScoringCandidate {
                id: CandidateId(7),
                text: "西安".into(),
            },
        ],
    }
}
fn caps() -> Capabilities {
    Capabilities {
        protocol: 1,
        tasks: vec![Task::Rank, Task::Predict],
        languages: vec!["zh".into()],
        max_context_chars: 256,
        max_candidates: 64,
        max_candidate_chars: 16,
        max_code_chars: 64,
        proposes_words: false,
        incremental: true,
        cooperative_cancel: false,
    }
}
fn response(r: &ScoreRequest) -> ScoreResponse {
    ScoreResponse {
        protocol: 1,
        key: r.key,
        scores: vec![
            CandidateScore {
                id: CandidateId(4),
                value: -2.0,
            },
            CandidateScore {
                id: CandidateId(7),
                value: -0.5,
            },
        ],
    }
}
fn control() -> ComputeControl {
    ComputeControl {
        cancellation: Cancellation::default(),
        deadline: Instant::now() + Duration::from_secs(2),
    }
}

#[test]
fn negotiates_tasks_language_unicode_limits_and_candidate_identity() {
    let mut r = request();
    r.validate(&caps()).unwrap();
    r.language = "unknown".into();
    assert_eq!(r.validate(&caps()), Err(Unavailable::Capability));
    r.language = "zh".into();
    r.prefix = "中".repeat(257);
    assert_eq!(r.validate(&caps()), Err(Unavailable::Limit));
    r.prefix.clear();
    r.candidates[1].id = r.candidates[0].id;
    assert_eq!(r.validate(&caps()), Err(Unavailable::Candidates));
}

#[test]
fn rejects_missing_duplicate_foreign_nonfinite_and_incompatible_scores() {
    let r = request();
    for scores in [
        vec![],
        vec![CandidateScore {
            id: CandidateId(4),
            value: 1.,
        }],
        vec![
            CandidateScore {
                id: CandidateId(4),
                value: 1.
            };
            2
        ],
        vec![CandidateScore {
            id: CandidateId(99),
            value: 1.,
        }],
        vec![CandidateScore {
            id: CandidateId(4),
            value: f32::NAN,
        }],
    ] {
        let mut result = response(&r);
        result.scores = scores;
        assert!(result.validate(&r, r.key).is_err());
    }
    let mut result = response(&r);
    result.protocol = 2;
    assert_eq!(result.validate(&r, r.key), Err(Unavailable::Protocol));
    response(&r).validate(&r, r.key).unwrap();
}

#[test]
fn every_identity_dimension_invalidates_old_response() {
    let r = request();
    for dimension in 0..12 {
        let mut current = r.key;
        match dimension {
            0 => current.edit.session += 1,
            1 => current.edit.generation += 1,
            2 => current.edit.editor += 1,
            3 => current.edit.epoch += 1,
            4 => current.edit.runtime += 1,
            5 => current.query.engine += 1,
            6 => current.query.version += 1,
            7 => current.view += 1,
            8 => current.context += 1,
            9 => current.model += 1,
            10 => current.config += 1,
            _ => current.request += 1,
        }
        assert_eq!(response(&r).validate(&r, current), Err(Unavailable::Stale));
    }
}

struct Fake {
    caps: Capabilities,
    resets: Arc<AtomicUsize>,
    fail: bool,
}
impl ModelBackend for Fake {
    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }
    fn score(
        &mut self,
        r: &ScoreRequest,
        _: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        if self.fail {
            Err(Unavailable::Inference)
        } else {
            Ok(response(r))
        }
    }
    fn reset(&mut self) {
        self.resets.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn sessions_reset_across_editors_errors_and_cancellation() {
    let resets = Arc::new(AtomicUsize::new(0));
    let mut session = ModelSession::new(Fake {
        caps: caps(),
        resets: resets.clone(),
        fail: false,
    });
    let mut r = request();
    session.score(&r, &control()).unwrap();
    session.score(&r, &control()).unwrap();
    assert_eq!(resets.load(Ordering::Relaxed), 1);
    r.key.edit.editor += 1;
    session.score(&r, &control()).unwrap();
    assert_eq!(resets.load(Ordering::Relaxed), 2);
    let c = control();
    c.cancellation.cancel();
    assert_eq!(session.score(&r, &c), Err(Unavailable::Cancelled));
    let mut c = control();
    c.deadline = Instant::now();
    assert_eq!(session.score(&r, &c), Err(Unavailable::Timeout));
    assert_eq!(resets.load(Ordering::Relaxed), 4);
}

#[test]
fn score_only_backend_supports_prediction_reranking_without_proposal() {
    let mut backend = Fake {
        caps: caps(),
        resets: Arc::default(),
        fail: false,
    };
    assert_eq!(
        backend.propose(
            &ProposalRequest {
                key: request().key,
                language: "zh".into(),
                prefix: "北京".into(),
                limit: 5
            },
            &control()
        ),
        Err(Unavailable::Capability)
    );
    let mut r = request();
    r.task = Task::Predict;
    r.code.clear();
    ModelSession::new(backend).score(&r, &control()).unwrap();
}

#[test]
fn context_edits_and_backend_failures_discard_internal_state() {
    let resets = Arc::new(AtomicUsize::new(0));
    let mut session = ModelSession::new(Fake {
        caps: caps(),
        resets: resets.clone(),
        fail: false,
    });
    let mut r = request();
    session.score(&r, &control()).unwrap();
    r.key.context += 1;
    session.score(&r, &control()).unwrap();
    assert_eq!(resets.load(Ordering::Relaxed), 2);
    let mut failing = ModelSession::new(Fake {
        caps: caps(),
        resets: resets.clone(),
        fail: true,
    });
    assert_eq!(failing.score(&r, &control()), Err(Unavailable::Inference));
    assert_eq!(resets.load(Ordering::Relaxed), 4);
}
