//! Runtime-independent model contract. Adapters own tokenization and recurrent state.
//! Scores are only comparable within one response; they are not probabilities.
pub mod context;
pub mod prediction;
pub mod ranking;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

use crate::{
    candidates::{CandidateId, QueryRevision},
    session::EditToken,
};
use serde::{Deserialize, Serialize};

/// Version of the auxiliary protocol, independent of the host edit protocol.
pub const PROTOCOL_VERSION: u32 = 1;

/// Rerank legal code candidates or rerank separately recalled next words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Task {
    Rank,
    Predict,
}

/// Opaque model/config/context generations must advance on replacement or edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestKey {
    pub edit: EditToken,
    pub query: QueryRevision,
    pub view: u64,
    pub context: u64,
    pub model: u64,
    pub config: u64,
    pub request: u64,
}

/// Text deliberately has no Debug implementation to discourage input logging.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoringCandidate {
    pub id: CandidateId,
    pub text: String,
}

/// Only confirmed text before the caret belongs in `prefix`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreRequest {
    pub protocol: u32,
    pub key: RequestKey,
    pub task: Task,
    pub language: String,
    pub prefix: String,
    pub code: String,
    pub candidates: Vec<ScoringCandidate>,
}

/// Limits use Unicode scalar counts; adapters additionally enforce token limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub protocol: u32,
    pub tasks: Vec<Task>,
    pub languages: Vec<String>,
    pub max_context_chars: usize,
    pub max_candidates: usize,
    pub max_candidate_chars: usize,
    pub max_code_chars: usize,
    pub proposes_words: bool,
    pub incremental: bool,
    /// False means cancellation only discards the result, not ongoing computation.
    pub cooperative_cancel: bool,
}

/// Machine-readable failures contain no input text or runtime debug dumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum Unavailable {
    #[error("incompatible auxiliary protocol")]
    Protocol,
    #[error("unsupported task or language")]
    Capability,
    #[error("input exceeds adapter limits")]
    Limit,
    #[error("model is missing")]
    MissingModel,
    #[error("unsupported model format, architecture or operator")]
    UnsupportedModel,
    #[error("model loading failed")]
    Load,
    #[error("inference failed")]
    Inference,
    #[error("no result")]
    Empty,
    #[error("prediction policy has not been supplied for this model")]
    Uncalibrated,
    #[error("prediction is below the model's confidence threshold")]
    LowConfidence,
    #[error("deadline exceeded")]
    Timeout,
    #[error("cancelled; computation may still be running")]
    Cancelled,
    #[error("request belongs to an obsolete session or version")]
    Stale,
    #[error("invalid candidate identities")]
    Candidates,
    #[error("non-finite score")]
    NonFinite,
    #[error("invalid single-word proposals")]
    Proposals,
}

impl ScoreRequest {
    /// Capability negotiation happens before any tokenization or computation.
    pub fn validate(&self, capabilities: &Capabilities) -> Result<(), Unavailable> {
        if self.protocol != PROTOCOL_VERSION || capabilities.protocol != PROTOCOL_VERSION {
            return Err(Unavailable::Protocol);
        }
        if !capabilities.tasks.contains(&self.task)
            || !capabilities.languages.contains(&self.language)
        {
            return Err(Unavailable::Capability);
        }
        if self.prefix.chars().count() > capabilities.max_context_chars
            || self.code.chars().count() > capabilities.max_code_chars
            || self.candidates.len() > capabilities.max_candidates
            || self
                .candidates
                .iter()
                .any(|c| c.text.chars().count() > capabilities.max_candidate_chars)
        {
            return Err(Unavailable::Limit);
        }
        if self.candidates.is_empty() {
            return Err(Unavailable::Empty);
        }
        let mut ids = HashSet::new();
        if self
            .candidates
            .iter()
            .any(|c| c.text.is_empty() || !ids.insert(c.id.0))
            || (self.task == Task::Rank && self.code.is_empty())
            || (self.task == Task::Predict && !self.code.is_empty())
        {
            return Err(Unavailable::Candidates);
        }
        Ok(())
    }
}

/// One finite score per requested identity. Larger values rank earlier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateScore {
    pub id: CandidateId,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreResponse {
    pub protocol: u32,
    pub key: RequestKey,
    pub scores: Vec<CandidateScore>,
    /// Package-specific admission after calibration; raw scores are not confidence.
    /// Missing on old/raw adapters: the product must not display model predictions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction_accepted: Option<Vec<CandidateId>>,
}

impl ScoreResponse {
    /// Validate again on the UI/session owner after asynchronous delivery.
    pub fn validate(&self, request: &ScoreRequest, current: RequestKey) -> Result<(), Unavailable> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(Unavailable::Protocol);
        }
        if self.key != request.key || self.key != current {
            return Err(Unavailable::Stale);
        }
        if self.scores.is_empty() {
            return Err(Unavailable::Empty);
        }
        let expected: HashSet<_> = request.candidates.iter().map(|c| c.id.0).collect();
        let mut seen = HashSet::new();
        for score in &self.scores {
            if !score.value.is_finite() {
                return Err(Unavailable::NonFinite);
            }
            if !expected.contains(&score.id.0) || !seen.insert(score.id.0) {
                return Err(Unavailable::Candidates);
            }
        }
        if seen != expected || expected.len() != request.candidates.len() {
            return Err(Unavailable::Candidates);
        }
        if let Some(accepted) = &self.prediction_accepted {
            let unique: HashSet<_> = accepted.iter().map(|id| id.0).collect();
            if request.task != Task::Predict
                || unique.len() != accepted.len()
                || !unique.is_subset(&expected)
            {
                return Err(Unavailable::Candidates);
            }
        }
        Ok(())
    }
}

/// Shared cancellation signal, usable without an async runtime.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Monotonic local deadline. IPC reconstructs it from a bounded remaining budget.
pub struct ComputeControl {
    pub cancellation: Cancellation,
    pub deadline: Instant,
}
impl ComputeControl {
    pub fn check(&self) -> Result<(), Unavailable> {
        if self.cancellation.is_cancelled() {
            Err(Unavailable::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(Unavailable::Timeout)
        } else {
            Ok(())
        }
    }
}

/// Optional single-word continuation proposal input (never full-sentence completion).
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalRequest {
    pub key: RequestKey,
    pub language: String,
    pub prefix: String,
    pub limit: usize,
}

impl ProposalRequest {
    pub fn validate(&self, caps: &Capabilities) -> Result<(), Unavailable> {
        if caps.protocol != PROTOCOL_VERSION {
            return Err(Unavailable::Protocol);
        }
        if !caps.proposes_words
            || !caps.tasks.contains(&Task::Predict)
            || !caps.languages.contains(&self.language)
        {
            return Err(Unavailable::Capability);
        }
        if self.limit == 0
            || self.limit > caps.max_candidates.min(64)
            || self.prefix.chars().count() > caps.max_context_chars.min(256)
        {
            return Err(Unavailable::Limit);
        }
        if self.prefix.trim().is_empty() {
            return Err(Unavailable::Empty);
        }
        Ok(())
    }
}

/// Complete short continuations, independent of all dictionaries. Array order
/// determines identity within `key.query`; no score is presented as accuracy.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalResponse {
    pub protocol: u32,
    pub key: RequestKey,
    pub words: Vec<String>,
}

/// Untagged wire union preserves old scoring responses while keeping proposal
/// identities and validation separate. Neither variant may contain host edits.
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PredictionResponse {
    Proposed(ProposalResponse),
    Scored(ScoreResponse),
}
impl ProposalResponse {
    pub fn validate(
        &self,
        request: &ProposalRequest,
        current: RequestKey,
    ) -> Result<(), Unavailable> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(Unavailable::Protocol);
        }
        if self.key != request.key || self.key != current {
            return Err(Unavailable::Stale);
        }
        if self.words.is_empty() {
            return Err(Unavailable::Empty);
        }
        if self.words.len() > request.limit.min(64) {
            return Err(Unavailable::Limit);
        }
        let mut seen = HashSet::new();
        if self
            .words
            .iter()
            .any(|word| !prediction::valid_proposal(&request.prefix, word) || !seen.insert(word))
        {
            return Err(Unavailable::Proposals);
        }
        Ok(())
    }
}

/// Implementations must isolate state to this owner. Switching owner/reset must discard
/// KV/LSTM state. Multi-token length normalization is an adapter/versioned package policy.
/// Errors must invalidate any partial state before the next call.
pub trait ModelBackend: Send {
    fn capabilities(&self) -> &Capabilities;
    fn score(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable>;
    fn reset(&mut self);
    /// A short decode of fixed, non-user text. Always discard its private state.
    fn warmup(&mut self, control: &ComputeControl) -> Result<(), Unavailable> {
        control.check()?;
        let task = if self.capabilities().tasks.contains(&Task::Rank) {
            Task::Rank
        } else {
            Task::Predict
        };
        let request = ScoreRequest {
            protocol: PROTOCOL_VERSION,
            key: RequestKey {
                edit: EditToken {
                    session: 0,
                    generation: 0,
                    editor: 0,
                    epoch: 0,
                    runtime: 0,
                },
                query: QueryRevision {
                    engine: 0,
                    version: 0,
                },
                view: 0,
                context: 0,
                model: 0,
                config: 0,
                request: 0,
            },
            task,
            language: "zh".into(),
            prefix: "预热".into(),
            code: if task == Task::Rank {
                "a".into()
            } else {
                String::new()
            },
            candidates: vec![ScoringCandidate {
                id: CandidateId(0),
                text: "完成".into(),
            }],
        };
        let result = self.score(&request, control).map(|_| ());
        self.reset();
        result.and_then(|()| control.check())
    }
    fn propose(
        &mut self,
        _request: &ProposalRequest,
        _control: &ComputeControl,
    ) -> Result<Vec<String>, Unavailable> {
        Err(Unavailable::Capability)
    }
}

impl<B: ModelBackend + ?Sized> ModelBackend for Box<B> {
    fn capabilities(&self) -> &Capabilities {
        (**self).capabilities()
    }
    fn score(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        (**self).score(request, control)
    }
    fn reset(&mut self) {
        (**self).reset();
    }
    fn warmup(&mut self, control: &ComputeControl) -> Result<(), Unavailable> {
        (**self).warmup(control)
    }
    fn propose(
        &mut self,
        request: &ProposalRequest,
        control: &ComputeControl,
    ) -> Result<Vec<String>, Unavailable> {
        (**self).propose(request, control)
    }
}

/// Central guard ensures a faulty adapter cannot bypass output checks or reuse state
/// across editors/models. It runs off the input thread; it cannot forcibly stop a backend.
pub struct ModelSession<B: ModelBackend> {
    backend: B,
    owner: Option<StateOwner>,
}
#[derive(PartialEq, Eq)]
struct StateOwner {
    session: u64,
    editor: u64,
    epoch: u64,
    runtime: u64,
    model: u64,
    config: u64,
    context: u64,
}
impl<B: ModelBackend> ModelSession<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            owner: None,
        }
    }
    pub fn reset(&mut self) {
        self.backend.reset();
        self.owner = None;
    }
    pub fn proposes_words(&self) -> bool {
        self.backend.capabilities().proposes_words
    }

    pub fn propose(
        &mut self,
        request: &ProposalRequest,
        control: &ComputeControl,
    ) -> Result<ProposalResponse, Unavailable> {
        let result = (|| {
            control.check()?;
            request.validate(self.backend.capabilities())?;
            // Generation starts from confirmed text only, never a previous branch.
            self.reset();
            let words = self.backend.propose(request, control)?;
            control.check()?;
            let response = ProposalResponse {
                protocol: PROTOCOL_VERSION,
                key: request.key,
                words,
            };
            response.validate(request, request.key)?;
            Ok(response)
        })();
        self.reset();
        result
    }
    pub fn score(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        let result = self.score_checked(request, control);
        if result.is_err() {
            self.reset();
        }
        result
    }
    fn score_checked(
        &mut self,
        request: &ScoreRequest,
        control: &ComputeControl,
    ) -> Result<ScoreResponse, Unavailable> {
        control.check()?;
        request.validate(self.backend.capabilities())?;
        let k = request.key;
        let owner = StateOwner {
            session: k.edit.session,
            editor: k.edit.editor,
            epoch: k.edit.epoch,
            runtime: k.edit.runtime,
            model: k.model,
            config: k.config,
            context: k.context,
        };
        if self.owner.as_ref() != Some(&owner) {
            self.reset();
            self.owner = Some(owner);
        }
        let result = self.backend.score(request, control)?;
        control.check()?;
        result.validate(request, request.key)?;
        Ok(result)
    }
}
