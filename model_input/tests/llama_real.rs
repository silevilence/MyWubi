#![cfg(feature = "llama-cpp-2")]
use core_engine::{
    assistance::*,
    candidates::{CandidateId, QueryRevision},
    session::EditToken,
};
use model_input::llama::LlamaScorer;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires MYWUBI_TEST_GGUF, run explicitly with the recorded model"]
fn real_cpu_scoring_preserves_identity_and_isolates_candidate_branches() {
    let path = std::env::var("MYWUBI_TEST_GGUF").expect("set MYWUBI_TEST_GGUF");
    let mut scorer = LlamaScorer::load(Path::new(&path), 1024, 2).unwrap();
    let mut request = ScoreRequest {
        protocol: 1,
        key: RequestKey {
            edit: EditToken {
                session: 1,
                generation: 1,
                editor: 1,
                epoch: 1,
                runtime: 1,
            },
            query: QueryRevision {
                engine: 1,
                version: 1,
            },
            view: 1,
            context: 1,
            model: 1,
            config: 1,
            request: 1,
        },
        task: Task::Rank,
        language: "zh".into(),
        prefix: "下周去".into(),
        code: "xian".into(),
        candidates: vec![
            ScoringCandidate {
                id: CandidateId(8),
                text: "西安".into(),
            },
            ScoringCandidate {
                id: CandidateId(2),
                text: "仙".into(),
            },
        ],
    };
    let control = ComputeControl {
        cancellation: Cancellation::default(),
        deadline: Instant::now() + Duration::from_secs(120),
    };
    let first = scorer.score(&request, &control).unwrap();
    first.validate(&request, request.key).unwrap();
    request.candidates.reverse();
    let reversed = scorer.score(&request, &control).unwrap();
    for score in &first.scores {
        let second = reversed.scores.iter().find(|s| s.id == score.id).unwrap();
        assert!((score.value - second.value).abs() < 1e-4);
    }
    scorer.reset();
    let reset = scorer.score(&request, &control).unwrap();
    assert_eq!(reversed, reset);
}

#[test]
fn missing_model_is_a_typed_error_before_runtime_initialization() {
    assert!(matches!(
        LlamaScorer::load(Path::new("does-not-exist.gguf"), 1024, 2),
        Err(Unavailable::MissingModel)
    ));
}

#[test]
fn invalid_model_format_is_rejected_without_scoring() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("evaluation/user.dict");
    assert!(matches!(
        LlamaScorer::load(&path, 1024, 2),
        Err(Unavailable::UnsupportedModel)
    ));
}
