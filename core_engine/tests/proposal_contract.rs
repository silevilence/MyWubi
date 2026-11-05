use core_engine::assistance::*;
use std::time::{Duration, Instant};

fn request() -> ProposalRequest {
    let score: ScoreRequest =
        serde_json::from_str(include_str!("../../model_input/evaluation/request.json")).unwrap();
    ProposalRequest {
        key: score.key,
        language: "zh".into(),
        prefix: "研究量子".into(),
        limit: 3,
    }
}
struct Generator {
    caps: Capabilities,
    words: Vec<String>,
}
impl ModelBackend for Generator {
    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }
    fn reset(&mut self) {}
    fn score(
        &mut self,
        _: &ScoreRequest,
        _: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        Err(Unavailable::Capability)
    }
    fn propose(
        &mut self,
        _: &ProposalRequest,
        _: &ComputeControl,
    ) -> Result<Vec<String>, Unavailable> {
        Ok(self.words.clone())
    }
}
fn backend(words: &[&str]) -> Generator {
    Generator {
        caps: Capabilities {
            protocol: 1,
            tasks: vec![Task::Predict],
            languages: vec!["zh".into()],
            max_context_chars: 256,
            max_candidates: 64,
            max_candidate_chars: 16,
            max_code_chars: 0,
            proposes_words: true,
            incremental: false,
            cooperative_cancel: true,
        },
        words: words.iter().map(|s| s.to_string()).collect(),
    }
}
fn control() -> ComputeControl {
    ComputeControl {
        cancellation: Cancellation::default(),
        deadline: Instant::now() + Duration::from_secs(1),
    }
}

#[test]
fn accepts_dictionary_independent_multichar_words_and_remainders() {
    let response = ModelSession::new(backend(&["纠缠态", "计算", "芯片"]))
        .propose(&request(), &control())
        .unwrap();
    assert_eq!(response.words, ["纠缠态", "计算", "芯片"]);
    let mut current = request().key;
    current.context += 1;
    assert_eq!(
        response.validate(&request(), current),
        Err(Unavailable::Stale)
    );
}
#[test]
fn rejects_duplicates_repeated_prefix_explanations_fragments_and_overlong_text() {
    for words in [
        vec!["计算", "计算"],
        vec!["量子"],
        vec!["研究量子计算"],
        vec!["可以续接：计算"],
        vec!["�"],
        vec!["这是超过十六个汉字的完整句子而不是单个词语"],
    ] {
        assert!(matches!(
            ModelSession::new(backend(&words)).propose(&request(), &control()),
            Err(Unavailable::Proposals)
        ));
    }
}
#[test]
fn scoring_only_empty_prefix_cancel_and_deadline_are_explicit() {
    let mut b = backend(&["计算"]);
    b.caps.proposes_words = false;
    assert!(matches!(
        ModelSession::new(b).propose(&request(), &control()),
        Err(Unavailable::Capability)
    ));
    let mut r = request();
    r.prefix.clear();
    assert!(matches!(
        ModelSession::new(backend(&["计算"])).propose(&r, &control()),
        Err(Unavailable::Empty)
    ));
    let mut c = control();
    c.cancellation.cancel();
    assert!(matches!(
        ModelSession::new(backend(&["计算"])).propose(&request(), &c),
        Err(Unavailable::Cancelled)
    ));
    c = control();
    c.deadline = Instant::now();
    assert!(matches!(
        ModelSession::new(backend(&["计算"])).propose(&request(), &c),
        Err(Unavailable::Timeout)
    ));
}
