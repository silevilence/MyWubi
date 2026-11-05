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
#[ignore = "requires the pinned Qwen2.5 GGUF; functional evidence, not quality"]
fn real_generation_proposes_dictionary_external_words_and_stops_without_late_results() {
    let path = std::env::var("MYWUBI_TEST_GGUF").unwrap();
    let policy: model_input::generation::GenerationPolicy =
        serde_json::from_str(include_str!("../evaluation/generation-policy.json")).unwrap();
    assert!(policy.validates(&model_input::package::sha256(Path::new(&path)).unwrap()));
    let backend = LlamaScorer::load(Path::new(&path), 1024, 2)
        .unwrap()
        .with_generation(policy);
    let mut session = ModelSession::new(backend);
    let template: ScoreRequest =
        serde_json::from_str(include_str!("../evaluation/request.json")).unwrap();
    let mut request = ProposalRequest {
        key: template.key,
        language: "zh".into(),
        prefix: "明天去".into(),
        limit: 3,
    };
    let mut control = ComputeControl {
        cancellation: Cancellation::default(),
        deadline: Instant::now() + Duration::from_secs(30),
    };
    let response = session.propose(&request, &control).unwrap();
    assert!(response.words.len() >= 2);
    let recalled =
        core_engine::assistance::prediction::Lexicon::new(&[]).recall(&request.prefix, 64);
    assert!(response
        .words
        .iter()
        .any(|word| !recalled.iter().any(|c| &c.text == word)));
    // Recall absence is weaker than dictionary absence. Check the latter too,
    // against every dictionary in this fixture (there is no user dictionary).
    request.prefix = "北京".into();
    request.limit = 1;
    let external = session.propose(&request, &control).unwrap();
    assert_eq!(external.words, ["市"]);
    assert!(
        !include_str!("../../core_engine/src/assistance/prediction_words.tsv")
            .lines()
            .any(|line| line.split('\t').next() == Some("市"))
    );
    assert!(
        !include_str!("../../core_engine/src/assistance/prediction_pairs.tsv")
            .lines()
            .any(|line| line.split('\t').nth(1) == Some("市"))
    );
    assert!(!include_str!("../../tables/wubi86.dict")
        .lines()
        .any(|line| line.split('\t').nth(1) == Some("市")));
    request.prefix = "今天的工作结束了。".into();
    assert!(matches!(
        session.propose(&request, &control),
        Err(Unavailable::Empty)
    ));
    request.prefix = "明天去".into();
    control.deadline = Instant::now() + Duration::from_millis(1);
    assert!(matches!(
        session.propose(&request, &control),
        Err(Unavailable::Timeout)
    ));
    control.deadline = Instant::now() + Duration::from_secs(30);
    let cancel = control.cancellation.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1));
        cancel.cancel();
    });
    assert!(matches!(
        session.propose(&request, &control),
        Err(Unavailable::Cancelled)
    ));
    thread.join().unwrap();
}

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
