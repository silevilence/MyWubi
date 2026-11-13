//! Two-phase host editing. Legacy processing advances before callbacks; touch waits for ack.
use crate::{candidates::*, runtime::RuntimeSnapshot, *};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

pub use crate::candidates::PreeditKind as DisplayAttrKind;

/// One operation remains one synchronous TSF edit session, including overflow.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EditSessionOp {
    CompositionUpdate {
        spelling: String,
        attr: DisplayAttrKind,
    },
    CommitAndReplace {
        text: String,
        replace_len: usize,
    },
    CommitThenUpdate {
        text: String,
        replace_len: usize,
        spelling: String,
        attr: DisplayAttrKind,
    },
    EndComposition {
        delete_text: bool,
    },
    NoOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EditTarget {
    OwnedComposition,
    CurrentSelection,
    LegacyRangeFallback,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InputPurpose {
    Text,
    Direct,
    Password,
}

/// Empty Enter is handed to the host with this already-normalized action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HostAction {
    Newline,
    Send,
    Search,
    Next,
    Done,
}

/// Platform supplies stable editor identity and explicit capabilities, not OS enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostContext {
    /// Platform privacy policy may disable assistance without changing normal input.
    #[serde(default = "assistance_allowed_by_default")]
    pub allows_assistance: bool,
    pub editor: u64,
    pub epoch: u64,
    pub supports_preedit: bool,
    pub supports_insert: bool,
    pub owns_preedit: bool,
    pub purpose: InputPurpose,
    pub selection_available: bool,
    pub surrounding_available: bool,
    pub enter_action: HostAction,
}
fn assistance_allowed_by_default() -> bool {
    true
}
impl Default for HostContext {
    fn default() -> Self {
        Self {
            allows_assistance: true,
            editor: 0,
            epoch: 0,
            supports_preedit: true,
            supports_insert: true,
            owns_preedit: false,
            purpose: InputPurpose::Text,
            selection_available: true,
            surrounding_available: false,
            enter_action: HostAction::Newline,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EditToken {
    pub session: u64,
    pub generation: u64,
    pub editor: u64,
    pub epoch: u64,
    pub runtime: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PreparedEdit {
    /// Dynamic outputs must not enter personal learning or candidate statistics.
    pub learnable: bool,
    pub token: EditToken,
    pub op: EditSessionOp,
    pub target: EditTarget,
    /// Preview only: callers must not publish this until acknowledgement succeeds.
    pub preview: ViewUpdate,
    pub disposition: Disposition,
}
impl PreparedEdit {
    pub fn consumed(&self) -> bool {
        self.disposition == Disposition::Handled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HostOutcome {
    Applied,
    RejectedBeforeWrite,
    /// Step count is retained for diagnostics; committed text is never replayed.
    PartiallyApplied {
        completed_steps: usize,
    },
    OutcomeUnknown,
    StaleContext,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionStatus {
    Ready,
    AwaitingAck,
    NeedsCleanup,
    Closed,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionError {
    Busy,
    Closed,
    NeedsCleanup,
    Rejected(Rejection),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AckStatus {
    Published,
    Retained,
    NeedsCleanup,
    Stale,
}

pub struct Acknowledged {
    pub status: AckStatus,
    pub follow_up: Option<PreparedEdit>,
}

struct Pending {
    local_query: bool,
    owned_preedit_before: bool,
    previous_prediction: Option<crate::assistance::prediction::Prediction>,
    predict_after: bool,
    plan: PreparedEdit,
    next: SemanticEngine,
    rollback: Option<SemanticEngine>,
    cleanup: bool,
}
struct PlanParts {
    op: EditSessionOp,
    view: SessionView,
    consumed: bool,
    unchanged: bool,
    reset_view: bool,
}

/// A caller serializes access, releases its lock for host calls, then acknowledges.
/// Legacy permits nested prepares; only the newest generation may publish.
pub struct Session {
    clock: Arc<dyn crate::shortcut_time::Clock>,
    personal_right: Option<String>,
    prediction_wait: Option<(crate::assistance::ScoreRequest, std::time::Instant)>,
    resources: Arc<RuntimeSnapshot>,
    prediction_dict: Arc<Dictionary>,
    prediction: Option<crate::assistance::prediction::Prediction>,
    prefix: crate::assistance::context::ContextBuffer,
    id: u64,
    engine: SemanticEngine,
    published_engine: SemanticEngine,
    config: Config,
    revision: u64,
    generation: u64,
    gate: bool,
    context: HostContext,
    view: SessionView,
    pending: Option<Pending>,
    cleanup: Option<bool>,
    closed: bool,
    deferred_runtime: Option<Arc<RuntimeSnapshot>>,
    last_outcome: Option<HostOutcome>,
}

impl Session {
    pub fn new(runtime: &RuntimeSnapshot, profile: BehaviorProfile, context: HostContext) -> Self {
        Self::new_with_clock(
            runtime,
            profile,
            context,
            Arc::new(crate::shortcut_time::UnavailableClock),
        )
    }
    pub fn new_with_clock(
        runtime: &RuntimeSnapshot,
        profile: BehaviorProfile,
        context: HostContext,
        clock: Arc<dyn crate::shortcut_time::Clock>,
    ) -> Self {
        let mut sm = runtime.state_machine();
        sm.set_clock(Arc::clone(&clock));
        let mut engine = SemanticEngine::new(sm, profile);
        let _ = engine.set_expression_context(
            context.purpose == InputPurpose::Text && context.allows_assistance,
        );
        let mut prefix = crate::assistance::context::ContextBuffer::new(256);
        prefix.bind(
            context.editor,
            context.epoch,
            (runtime.config.prediction.enabled || runtime.config.personal.predict)
                && context.purpose == InputPurpose::Text
                && context.allows_assistance,
        );
        Self {
            clock,
            prediction_wait: None,
            personal_right: None,
            resources: Arc::new(runtime.clone()),
            prediction_dict: Arc::clone(&runtime.dict),
            prediction: None,
            prefix,
            id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            published_engine: engine.clone(),
            engine,
            config: runtime.config.clone(),
            revision: runtime.revision,
            generation: 0,
            gate: true,
            context,
            view: SessionView::default(),
            pending: None,
            cleanup: None,
            closed: false,
            deferred_runtime: None,
            last_outcome: None,
        }
    }
    pub fn state_machine(&self) -> &StateMachine {
        self.engine.state_machine()
    }
    pub fn base_mode(&self) -> BaseMode {
        self.engine.base_mode()
    }
    pub fn temporary_mode(&self) -> Option<TemporaryMode> {
        self.engine.temporary_mode()
    }
    pub fn capability(&self, mode: TemporaryMode) -> Capability {
        self.engine.capability(mode)
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn input_gate(&self) -> bool {
        self.gate
    }
    pub fn context(&self) -> HostContext {
        self.context
    }
    pub fn snapshot(&self) -> &SessionView {
        &self.view
    }
    pub fn last_outcome(&self) -> Option<HostOutcome> {
        self.last_outcome
    }
    pub fn candidate_window(&self, offset: usize, count: usize) -> CandidateWindow {
        if let Some(prediction) = &self
            .prediction
            .as_ref()
            .filter(|_| self.prediction_wait.is_none())
        {
            return prediction.window(offset, count);
        }
        self.published_engine.candidate_window(offset, count)
    }
    /// IDs for the exact currently published desktop page, including mixed continuations.
    pub fn visible_candidate_window(&self) -> CandidateWindow {
        let offset = if self.has_prediction() {
            0
        } else {
            self.published_engine.state_machine().candidate_offset()
        };
        self.candidate_window(offset, self.view.items.len())
    }
    pub fn has_prediction(&self) -> bool {
        self.prediction.is_some() && self.prediction_wait.is_none()
    }
    pub fn waiting_for_prediction(&self) -> bool {
        self.prediction_wait.is_some()
    }
    pub fn set_assistance_allowed(&mut self, allowed: bool) {
        if self.context.allows_assistance != allowed {
            self.clear_prediction_context();
            self.context.allows_assistance = allowed;
            let expressions_allowed = allowed && self.context.purpose == InputPurpose::Text;
            let _ = self.engine.set_expression_context(expressions_allowed);
            if let Some(transition) = self
                .published_engine
                .set_expression_context(expressions_allowed)
            {
                self.view = plan(
                    self.published_engine.state_machine(),
                    self.config.basic.show_code_hints,
                    transition,
                    &self.view,
                )
                .view;
            }
            // A privacy notification can arrive while a host edit awaits confirmation.
            // Refresh each snapshot separately; never publish its unconfirmed input.
            if let Some(pending) = &mut self.pending {
                if let Some(transition) = pending.next.set_expression_context(expressions_allowed) {
                    pending.plan.preview = ViewUpdate::Replace(
                        plan(
                            pending.next.state_machine(),
                            self.config.basic.show_code_hints,
                            transition,
                            &self.view,
                        )
                        .view,
                    );
                }
                if let Some(rollback) = &mut pending.rollback {
                    let _ = rollback.set_expression_context(expressions_allowed);
                }
            }
            self.prefix.bind(
                self.context.editor,
                self.context.epoch,
                (self.config.prediction.enabled || self.config.personal.predict)
                    && allowed
                    && self.context.purpose == InputPurpose::Text,
            );
        }
    }
    /// Call for external edits, cursor movement or keys handled outside this session.
    pub fn clear_prediction_context(&mut self) {
        self.personal_right = None;
        self.prediction_wait = None;
        self.prefix.invalidate();
        self.prediction = None;
        if self.view.prediction {
            self.view = SessionView::default();
        }
    }
    /// Only authoritative text before the caret, excluding preedit, is accepted.
    pub fn observe_prefix(&mut self, editor: u64, epoch: u64, text: Option<&str>) {
        if (self.config.prediction.enabled || self.config.personal.predict)
            && self.context.purpose == InputPurpose::Text
            && self.context.allows_assistance
        {
            let previous = self.prefix.prefix().to_owned();
            self.prefix.observe(editor, epoch, text);
            if self.prediction.is_some() && previous != self.prefix.prefix() {
                self.prediction_wait = None;
                self.prediction = None;
                self.view = SessionView::default();
            }
        }
    }
    fn publish_predictions(&mut self) {
        if !(self.config.prediction.enabled || self.config.personal.predict)
            || !self.gate
            || self.context.purpose != InputPurpose::Text
            || !self.context.allows_assistance
            || self.engine.base_mode() != BaseMode::Main
            || self.engine.temporary_mode().is_some()
            || self.state_machine().mode() != InputMode::Main
            || !self.state_machine().spelling().is_empty()
        {
            return;
        }
        let items = self.prediction_dict.prediction_lexicon().recall(
            self.prefix.prefix(),
            usize::from(
                if self.config.prediction.model.is_some() || self.config.personal.predict {
                    self.config.prediction.max_candidates
                } else {
                    self.config.basic.candidate_count
                },
            ),
        );
        if !items.is_empty()
            || ((self.config.prediction.model.is_some() || self.config.personal.predict)
                && !self.prefix.prefix().trim().is_empty())
        {
            let prediction = crate::assistance::prediction::Prediction::new(items);
            if self.config.prediction.model.is_some() || self.config.personal.predict {
                use crate::assistance::*;
                let request = ScoreRequest {
                    protocol: PROTOCOL_VERSION,
                    key: RequestKey {
                        edit: self.token(),
                        query: prediction.query,
                        view: prediction.query.version,
                        context: self.prefix.generation(),
                        model: self.revision,
                        config: self.revision,
                        request: prediction.query.version,
                    },
                    task: Task::Predict,
                    language: "zh".into(),
                    prefix: self.prefix.prefix().into(),
                    code: String::new(),
                    candidates: prediction
                        .items
                        .iter()
                        .map(|c| ScoringCandidate {
                            id: c.id,
                            text: c.text.clone(),
                        })
                        .collect(),
                };
                self.prediction_wait = Some((
                    request,
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(u64::from(
                            self.config.prediction.wait_ms,
                        )),
                ));
                self.view = SessionView::default();
            } else {
                self.view = prediction.view();
            }
            self.prediction = Some(prediction);
        }
    }
    /// A snapshot of pending model work; adapters enqueue it once off the input path.
    pub fn prediction_request(&self) -> Option<crate::assistance::ScoreRequest> {
        self.prediction_wait
            .as_ref()
            .map(|(request, _)| request.clone())
    }
    pub fn model_store_path(&self) -> std::path::PathBuf {
        crate::config_path::model_store_path(&self.resources.config_path)
    }
    pub fn personal_store_path(&self) -> std::path::PathBuf {
        self.resources
            .config_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("personal")
    }
    pub fn personal_right(&self) -> Option<&str> {
        self.personal_right.as_deref()
    }
    pub fn observe_personal_right(&mut self, right: Option<&str>) {
        if self.config.personal.predict && self.personal_right.as_deref() != right {
            self.clear_prediction_context();
        }
        self.personal_right = right.map(str::to_owned);
    }
    pub fn expire_prediction(&mut self) -> bool {
        if self
            .prediction_wait
            .as_ref()
            .is_some_and(|(_, deadline)| std::time::Instant::now() >= *deadline)
        {
            let key = self.prediction_wait.as_ref().unwrap().0.key;
            self.complete_prediction(key, Err(crate::assistance::Unavailable::Timeout))
        } else {
            false
        }
    }
    /// Publish exactly once before interaction starts. A late score never reorders
    /// an already displayed prediction; selection retains immutable candidate IDs.
    pub fn complete_prediction(
        &mut self,
        key: crate::assistance::RequestKey,
        result: Result<crate::assistance::ScoreResponse, crate::assistance::Unavailable>,
    ) -> bool {
        self.complete_model_prediction(
            key,
            result.map(crate::assistance::PredictionResponse::Scored),
        )
    }
    pub fn complete_model_prediction(
        &mut self,
        key: crate::assistance::RequestKey,
        result: Result<crate::assistance::PredictionResponse, crate::assistance::Unavailable>,
    ) -> bool {
        if self
            .prediction_wait
            .as_ref()
            .is_none_or(|(r, _)| r.key != key)
        {
            return false;
        }
        let Some((mut request, deadline)) = self.prediction_wait.take() else {
            return false;
        };
        if self.token() != request.key.edit || self.prefix.generation() != request.key.context {
            self.prediction = None;
            return false;
        }
        let Some(prediction) = &mut self.prediction else {
            return false;
        };
        if prediction.query != request.key.query {
            return false;
        }
        // Generated words enter only this post-commit view, never the code
        // dictionary. The entire result is checked before replacing any items.
        let response = result.and_then(|response| {
            use crate::assistance::*;
            match response {
                PredictionResponse::Scored(response) => Ok(response),
                PredictionResponse::Proposed(response) => {
                    response.validate(
                        &ProposalRequest {
                            key: request.key,
                            prefix: request.prefix.clone(),
                            language: request.language.clone(),
                            limit: usize::from(self.config.prediction.max_candidates),
                        },
                        request.key,
                    )?;
                    prediction.items = response
                        .words
                        .into_iter()
                        .enumerate()
                        .map(|(i, text)| crate::candidates::Candidate {
                            expression_kind: None,
                            id: crate::candidates::CandidateId(i),
                            text,
                            exact: true,
                            hints: vec![],
                        })
                        .collect();
                    request.candidates = prediction
                        .items
                        .iter()
                        .map(|c| ScoringCandidate {
                            id: c.id,
                            text: c.text.clone(),
                        })
                        .collect();
                    Ok(ScoreResponse {
                        protocol: PROTOCOL_VERSION,
                        key: request.key,
                        scores: request
                            .candidates
                            .iter()
                            .enumerate()
                            .map(|(i, c)| CandidateScore {
                                id: c.id,
                                value: -(i as f32),
                            })
                            .collect(),
                        prediction_accepted: Some(
                            request.candidates.iter().map(|c| c.id).collect(),
                        ),
                    })
                }
            }
        });
        let response = response.ok().filter(|response| {
            std::time::Instant::now() < deadline
                && response.validate(&request, request.key).is_ok()
                && response
                    .prediction_accepted
                    .as_ref()
                    .is_some_and(|ids| !ids.is_empty())
        });
        let Some(response) = response else {
            self.prediction = None;
            self.view = SessionView::default();
            return true;
        };
        prediction.items.retain(|candidate| {
            response
                .prediction_accepted
                .as_ref()
                .is_some_and(|ids| ids.contains(&candidate.id))
        });
        prediction.items.sort_by(|a, b| {
            let score = |id| {
                response
                    .scores
                    .iter()
                    .find(|s| s.id == id)
                    .map_or(f32::NEG_INFINITY, |s| s.value)
            };
            score(b.id).total_cmp(&score(a.id))
        });
        prediction
            .items
            .truncate(usize::from(self.config.basic.candidate_count));
        self.view = prediction.view();
        true
    }
    pub fn status(&self) -> SessionStatus {
        if self.closed {
            SessionStatus::Closed
        } else if self.cleanup.is_some() {
            SessionStatus::NeedsCleanup
        } else if self.pending.is_some() {
            SessionStatus::AwaitingAck
        } else {
            SessionStatus::Ready
        }
    }
    fn legacy(&self) -> bool {
        self.engine.profile == BehaviorProfile::LegacyV1
    }
    /// Explicit selection keys refer to the currently published prediction list.
    /// Hidden model work must never turn an ordinary key into a selection.
    fn prediction_key_index(&self, action: &Action) -> Option<usize> {
        if !self.has_prediction() {
            return None;
        }
        let index = match action {
            Action::Space | Action::CommitPreferred | Action::Legacy(InputEvent::Space) => 0,
            Action::Legacy(InputEvent::Select(index)) => index.checked_sub(1)?,
            Action::InputCode(c)
            | Action::InputText(c, TextIntent::Normal)
            | Action::Legacy(InputEvent::Char(c))
                if ('1'..='9').contains(c) =>
            {
                (*c as usize) - ('1' as usize)
            }
            _ => return None,
        };
        self.prediction.as_ref()?.items.get(index).map(|_| index)
    }
    pub fn would_handle(&self, action: &Action) -> bool {
        if self.has_prediction()
            && self.gate
            && !self.closed
            && (self.prediction_key_index(action).is_some()
                || matches!(
                    action,
                    Action::SelectCandidate(..)
                        | Action::Cancel
                        | Action::Legacy(InputEvent::Select(_) | InputEvent::Esc)
                ))
        {
            return true;
        }
        if !self.legacy() && self.context.purpose != InputPurpose::Text {
            return !self.closed
                && self.cleanup.is_none()
                && self.gate
                && self.context.supports_insert
                && match action {
                    Action::InputText(..)
                    | Action::InputCode(_)
                    | Action::InputSymbol(_)
                    | Action::Legacy(InputEvent::Char(_) | InputEvent::Symbol(_)) => true,
                    Action::InsertLiteral(text) => !text.is_empty() && text.len() <= 65536,
                    _ => false,
                };
        }
        !self.closed && self.cleanup.is_none() && self.gate && self.engine.would_handle(action)
    }
    pub fn would_handle_legacy(&self, event: &InputEvent, system_modifier: bool) -> bool {
        if !system_modifier
            && self.has_prediction()
            && (self
                .prediction_key_index(&Action::Legacy(event.clone()))
                .is_some()
                || matches!(event, InputEvent::Select(_) | InputEvent::Esc))
        {
            return true;
        }
        if system_modifier || !self.would_handle(&Action::Legacy(event.clone())) {
            return false;
        }
        let sm = self.state_machine();
        if sm.mode() != InputMode::Main
            || matches!(event, InputEvent::Char(c) | InputEvent::Symbol(c) if sm.is_temporary_trigger(*c))
        {
            return true;
        }
        match event {
            InputEvent::Char(c) | InputEvent::Symbol(c) if sm.accepts_code_char(*c) => true,
            InputEvent::Symbol(c) if !c.is_ascii() => true,
            _ => !sm.spelling().is_empty(),
        }
    }
    fn token(&self) -> EditToken {
        EditToken {
            session: self.id,
            generation: self.generation,
            editor: self.context.editor,
            epoch: self.context.epoch,
            runtime: self.revision,
        }
    }
    /// Check immediately before executing host writes, as well as before publishing their result.
    pub fn is_current(&self, token: EditToken) -> bool {
        !self.closed
            && token == self.token()
            && self.pending.as_ref().is_some_and(|p| p.plan.token == token)
    }

    pub fn prepare(&mut self, action: Action) -> Result<PreparedEdit, SessionError> {
        // Input always wins over unshown model suggestions, including number/space keys.
        if self.prediction_wait.take().is_some() {
            self.prediction = None;
        }
        if self.closed {
            return Err(SessionError::Closed);
        }
        if self.cleanup.is_some() {
            return Err(SessionError::NeedsCleanup);
        }
        if !self.legacy() && self.pending.is_some() {
            return Err(SessionError::Busy);
        }
        let predict_after = self.engine.base_mode() == BaseMode::Main
            && self.engine.temporary_mode().is_none()
            && self.state_machine().mode() == InputMode::Main;
        let dismiss = self.prediction.is_some()
            && matches!(action, Action::Cancel | Action::Legacy(InputEvent::Esc));
        let action = if let Some(prediction) = &self.prediction {
            let selected = match &action {
                Action::SelectCandidate(query, id) => {
                    if *query != prediction.query {
                        return Err(SessionError::Rejected(Rejection::StaleCandidate));
                    }
                    Some(
                        prediction
                            .items
                            .iter()
                            .position(|item| item.id == *id)
                            .ok_or(SessionError::Rejected(Rejection::StaleCandidate))?,
                    )
                }
                Action::Legacy(InputEvent::Select(index)) => Some(
                    index
                        .checked_sub(1)
                        .ok_or(SessionError::Rejected(Rejection::StaleCandidate))?,
                ),
                _ => self.prediction_key_index(&action),
            };
            if let Some(index) = selected {
                let candidate = prediction
                    .items
                    .get(index)
                    .ok_or(SessionError::Rejected(Rejection::StaleCandidate))?;
                Action::InsertLiteral(candidate.text.clone())
            } else {
                action
            }
        } else {
            action
        };
        if matches!(
            action,
            Action::Backspace
                | Action::Space
                | Action::Enter
                | Action::SetBaseMode(_)
                | Action::BeginTemporary(_)
                | Action::Legacy(InputEvent::Backspace | InputEvent::Space | InputEvent::Enter)
        ) && self.state_machine().spelling().is_empty()
        {
            self.prefix.invalidate();
        }
        let mut next = self.engine.clone();
        let rollback = (self.legacy()
            && (matches!(action, Action::Legacy(_) | Action::SelectCandidate(_, _))
                || self.state_machine().mode().expression_kind().is_some())
            && self.state_machine().mode() != InputMode::Main)
            .then(|| self.engine.clone());
        let action = if !self.legacy() && self.context.purpose != InputPurpose::Text {
            match action {
                Action::InputText(c, _)
                | Action::InputCode(c)
                | Action::InputSymbol(c)
                | Action::Legacy(InputEvent::Char(c) | InputEvent::Symbol(c)) => {
                    Action::InsertLiteral(c.to_string())
                }
                Action::InsertLiteral(text) => Action::InsertLiteral(text),
                Action::Space
                | Action::Enter
                | Action::Backspace
                | Action::Cancel
                | Action::CommitPreferred
                | Action::CommitRaw
                | Action::Legacy(
                    InputEvent::Space | InputEvent::Enter | InputEvent::Backspace | InputEvent::Esc,
                ) => {
                    return self.stage(
                        next,
                        None,
                        PlanParts {
                            op: EditSessionOp::NoOp,
                            view: self.view.clone(),
                            consumed: false,
                            unchanged: true,
                            reset_view: false,
                        },
                        false,
                    );
                }
                _ => {
                    return Err(SessionError::Rejected(Rejection::Unavailable(
                        "当前输入框只接受字面文本".into(),
                    )))
                }
            }
        } else {
            action
        };
        let result = if self.gate {
            next.dispatch(action)
        } else {
            ActionResult {
                disposition: Disposition::NotHandled,
                transition: Transition::None,
            }
        };
        if let Disposition::Rejected(reason) = result.disposition {
            return Err(SessionError::Rejected(reason));
        }
        let current_view = if self.prediction.is_some() {
            SessionView::default()
        } else {
            self.view.clone()
        };
        let mut parts = plan(
            next.state_machine(),
            self.config.basic.show_code_hints,
            result.transition,
            &current_view,
        );
        parts.consumed = dismiss || result.disposition == Disposition::Handled;
        if self.prediction.is_some() {
            parts.unchanged = false;
        }
        let prepared = self.stage(next, rollback, parts, false)?;
        if let Some(pending) = &mut self.pending {
            pending.predict_after = predict_after && pending.plan.learnable;
        }
        Ok(prepared)
    }

    fn stage(
        &mut self,
        next: SemanticEngine,
        rollback: Option<SemanticEngine>,
        mut parts: PlanParts,
        cleanup: bool,
    ) -> Result<PreparedEdit, SessionError> {
        let local_query = self.state_machine().mode().expression_kind().is_some()
            || next.state_machine().mode().expression_kind().is_some();
        if self.closed {
            return Err(SessionError::Closed);
        }
        let target = if self.state_machine().mode().expression_kind().is_some() {
            EditTarget::CurrentSelection
        } else if self.legacy() {
            EditTarget::LegacyRangeFallback
        } else if self.context.owns_preedit {
            EditTarget::OwnedComposition
        } else {
            EditTarget::CurrentSelection
        };
        if !self.legacy() {
            if !self.context.supports_insert
                && matches!(
                    parts.op,
                    EditSessionOp::CommitAndReplace { .. }
                        | EditSessionOp::CommitThenUpdate { .. }
                        | EditSessionOp::CompositionUpdate { .. }
                )
            {
                return Err(SessionError::Rejected(Rejection::Unavailable(
                    "宿主不支持文本编辑".into(),
                )));
            }
            if let EditSessionOp::CommitAndReplace { replace_len, .. }
            | EditSessionOp::CommitThenUpdate { replace_len, .. } = &mut parts.op
            {
                *replace_len = 0;
            }
            if !self.context.supports_preedit {
                parts.op = match parts.op {
                    EditSessionOp::CompositionUpdate { .. }
                    | EditSessionOp::EndComposition { .. } => EditSessionOp::NoOp,
                    EditSessionOp::CommitThenUpdate { text, .. } => {
                        EditSessionOp::CommitAndReplace {
                            text,
                            replace_len: 0,
                        }
                    }
                    other => other,
                };
            }
            if !self.context.owns_preedit
                && matches!(parts.op, EditSessionOp::EndComposition { .. })
            {
                parts.op = EditSessionOp::NoOp;
            }
            if self.context.purpose == InputPurpose::Password {
                parts.view = SessionView::default();
            }
        }
        self.generation += 1;
        if self.legacy() {
            self.engine = next.clone();
        }
        let plan = PreparedEdit {
            learnable: !next.state_machine().committed_shortcut(),
            token: self.token(),
            op: parts.op,
            target,
            preview: if parts.reset_view {
                ViewUpdate::Reset(parts.view)
            } else if parts.unchanged {
                ViewUpdate::Unchanged
            } else {
                ViewUpdate::Replace(parts.view)
            },
            disposition: if parts.consumed {
                Disposition::Handled
            } else {
                Disposition::NotHandled
            },
        };
        self.pending = Some(Pending {
            local_query,
            owned_preedit_before: self.context.owns_preedit,
            previous_prediction: self.prediction.take(),
            predict_after: false,
            plan: plan.clone(),
            next,
            rollback,
            cleanup,
        });
        Ok(plan)
    }

    pub fn acknowledge(&mut self, token: EditToken, outcome: HostOutcome) -> Acknowledged {
        let owns_preedit = self.context.owns_preedit
            && !matches!(
                outcome,
                HostOutcome::OutcomeUnknown
                    | HostOutcome::PartiallyApplied { .. }
                    | HostOutcome::StaleContext
            );
        self.acknowledge_with_ownership(token, outcome, owns_preedit)
    }

    /// Touch adapters must reconfirm ownership of the same range after uncertain
    /// host effects. The default acknowledge conservatively relinquishes it.
    pub fn acknowledge_with_ownership(
        &mut self,
        token: EditToken,
        outcome: HostOutcome,
        owns_preedit: bool,
    ) -> Acknowledged {
        if !self.is_current(token) {
            return Acknowledged {
                status: AckStatus::Stale,
                follow_up: None,
            };
        }
        if !self.legacy() {
            self.context.owns_preedit = owns_preedit;
        }
        let Some(pending) = self.pending.take() else {
            return Acknowledged {
                status: AckStatus::Stale,
                follow_up: None,
            };
        };
        self.last_outcome = Some(outcome);
        let mut confirmed = pending.plan.op.clone();
        if pending.owned_preedit_before {
            // Replacing our preedit does not delete already confirmed context.
            match &mut confirmed {
                EditSessionOp::CommitAndReplace { replace_len, .. }
                | EditSessionOp::CommitThenUpdate { replace_len, .. } => *replace_len = 0,
                _ => {}
            }
        }
        if pending.plan.learnable {
            self.prefix.acknowledged(token, &confirmed, outcome);
        } else if outcome != HostOutcome::RejectedBeforeWrite {
            self.prefix.invalidate();
        }
        let committed = pending.predict_after
            && matches!(&pending.plan.op, EditSessionOp::CommitAndReplace { text, .. } | EditSessionOp::CommitThenUpdate { text, .. } if !text.is_empty());
        let status = match outcome {
            HostOutcome::Applied => {
                self.engine = pending.next;
                self.published_engine = self.engine.clone();
                if let ViewUpdate::Replace(view) | ViewUpdate::Reset(view) = pending.plan.preview {
                    self.view = view;
                }
                self.cleanup = None;
                match pending.plan.op {
                    EditSessionOp::CompositionUpdate { .. }
                    | EditSessionOp::CommitThenUpdate { .. } => {
                        self.context.owns_preedit = self.context.supports_preedit
                    }
                    EditSessionOp::EndComposition { .. }
                    | EditSessionOp::CommitAndReplace { .. } => self.context.owns_preedit = false,
                    _ => {}
                }
                if committed {
                    self.publish_predictions();
                }
                AckStatus::Published
            }
            HostOutcome::RejectedBeforeWrite
                if !pending.cleanup
                    && (!self.legacy()
                        || pending.rollback.is_some()
                        || pending.previous_prediction.is_some()) =>
            {
                if let Some(previous) = pending.rollback {
                    self.engine = previous;
                }
                self.prediction = pending.previous_prediction;
                AckStatus::Retained
            }
            HostOutcome::StaleContext => {
                self.invalidate_context(self.context, false);
                AckStatus::Stale
            }
            _ => {
                self.engine.reset_context();
                self.engine.invalidate_query();
                self.published_engine = self.engine.clone();
                self.view = SessionView::default();
                let delete = self.cleanup.unwrap_or(matches!(
                    pending.plan.op,
                    EditSessionOp::EndComposition { delete_text: true }
                ));
                self.cleanup = ((self.legacy() && !pending.local_query)
                    || self.context.owns_preedit)
                    .then_some(delete);
                if self.cleanup.is_some() {
                    AckStatus::NeedsCleanup
                } else {
                    AckStatus::Retained
                }
            }
        };
        let follow_up = self
            .deferred_runtime
            .take()
            .and_then(|runtime| self.apply_runtime(runtime).ok().flatten());
        Acknowledged { status, follow_up }
    }

    fn cleared(
        &mut self,
        next: SemanticEngine,
        op: EditSessionOp,
        cleanup: bool,
    ) -> Result<PreparedEdit, SessionError> {
        self.stage(
            next,
            None,
            PlanParts {
                op,
                view: SessionView::default(),
                consumed: true,
                unchanged: false,
                reset_view: true,
            },
            cleanup,
        )
    }
    pub fn recover(&mut self) -> Result<Option<PreparedEdit>, SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        let Some(delete_text) = self.cleanup else {
            return Ok(None);
        };
        self.cleared(
            self.engine.clone(),
            EditSessionOp::EndComposition { delete_text },
            true,
        )
        .map(Some)
    }
    /// Gate is the Windows OS bypass switch, distinct from persistent English lookup.
    pub fn set_input_gate(&mut self, enabled: bool) -> Result<Option<PreparedEdit>, SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if self.gate == enabled {
            return Ok(None);
        }
        self.gate = enabled;
        self.clear_prediction_context();
        if enabled {
            return Ok(None);
        }
        let mut next = self.engine.clone();
        let temporary =
            next.temporary_mode().is_some() || next.state_machine().mode() == InputMode::Menu;
        let op = if next.state_machine().mode().expression_kind().is_some() {
            EditSessionOp::NoOp
        } else if temporary {
            EditSessionOp::EndComposition { delete_text: true }
        } else if !next.state_machine().spelling().is_empty() {
            EditSessionOp::CommitAndReplace {
                text: next.state_machine().spelling().to_owned(),
                replace_len: next.state_machine().spelling().chars().count(),
            }
        } else {
            EditSessionOp::EndComposition { delete_text: false }
        };
        next.reset_context();
        next.invalidate_query();
        self.cleared(next, op, false).map(Some)
    }
    pub fn lose_focus(&mut self) -> Result<PreparedEdit, SessionError> {
        self.clear_prediction_context();
        let temporary = self.engine.temporary_mode().is_some()
            || self.state_machine().mode() == InputMode::Menu;
        let op = self.finish_operation(temporary);
        let mut next = self.engine.clone();
        next.reset_context();
        next.invalidate_query();
        self.cleared(next, op, false)
    }

    fn finish_operation(&self, temporary: bool) -> EditSessionOp {
        if self.state_machine().mode().expression_kind().is_some() {
            return EditSessionOp::NoOp;
        }
        // Local preedit has no host range to finish: preserve a base round by
        // inserting its raw text while the old connection is still valid.
        if !self.legacy()
            && !self.context.supports_preedit
            && !temporary
            && !self.state_machine().spelling().is_empty()
        {
            EditSessionOp::CommitAndReplace {
                text: self.state_machine().spelling().to_owned(),
                replace_len: 0,
            }
        } else {
            EditSessionOp::EndComposition {
                delete_text: temporary,
            }
        }
    }
    /// A dead connection/terminated composition cannot be cleaned through the new selection.
    pub fn invalidate_context(&mut self, context: HostContext, preserve_base: bool) {
        self.clear_prediction_context();
        self.prefix.bind(
            context.editor,
            context.epoch,
            (self.config.prediction.enabled || self.config.personal.predict)
                && context.purpose == InputPurpose::Text
                && context.allows_assistance,
        );
        self.generation += 1;
        self.pending = None;
        self.cleanup = None;
        self.context = context;
        if preserve_base {
            self.engine.reset_context();
        } else {
            let mut sm = self.state_machine().clone();
            sm.reset();
            self.engine = SemanticEngine::new(sm, self.engine.profile);
        }
        self.engine.invalidate_query();
        if let Some(runtime) = self.deferred_runtime.take() {
            let base = self.engine.base_mode();
            let mut sm = runtime.state_machine();
            sm.set_clock(Arc::clone(&self.clock));
            self.engine = SemanticEngine::new(sm, self.engine.profile);
            if preserve_base && base == BaseMode::English {
                self.engine.dispatch(Action::SetBaseMode(BaseMode::English));
            }
            self.config = runtime.config.clone();
            self.resources = Arc::clone(&runtime);
            self.prediction_dict = Arc::clone(&runtime.dict);
            self.revision = runtime.revision;
        }
        let _ = self.engine.set_expression_context(
            context.purpose == InputPurpose::Text && context.allows_assistance,
        );
        self.published_engine = self.engine.clone();
        self.view = SessionView::default();
        self.prefix.bind(
            context.editor,
            context.epoch,
            (self.config.prediction.enabled || self.config.personal.predict)
                && context.purpose == InputPurpose::Text
                && context.allows_assistance,
        );
    }
    pub fn host_terminated(&mut self) {
        let mut context = self.context;
        context.owns_preedit = false;
        context.epoch += 1;
        self.invalidate_context(context, !self.legacy());
    }
    pub fn close(&mut self) {
        self.host_terminated();
        self.closed = true;
        self.deferred_runtime = None;
    }

    pub fn apply_runtime(
        &mut self,
        runtime: Arc<RuntimeSnapshot>,
    ) -> Result<Option<PreparedEdit>, SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if runtime.revision == self.revision {
            return Ok(None);
        }
        if !self.legacy()
            && (runtime.revision < self.revision
                || self
                    .deferred_runtime
                    .as_ref()
                    .is_some_and(|r| r.revision > runtime.revision))
        {
            return Ok(None);
        }
        if !self.legacy() && self.pending.is_some() {
            self.deferred_runtime = Some(runtime);
            return Ok(None);
        }
        if self.pending.is_none() && self.only_prediction_changed(&runtime) {
            self.config = runtime.config.clone();
            self.revision = runtime.revision;
            self.resources = Arc::clone(&runtime);
            self.prediction_dict = Arc::clone(&runtime.dict);
            self.clear_prediction_context();
            self.prefix.bind(
                self.context.editor,
                self.context.epoch,
                (self.config.prediction.enabled || self.config.personal.predict)
                    && self.context.allows_assistance
                    && self.context.purpose == InputPurpose::Text,
            );
            self.generation += 1;
            self.engine.invalidate_query();
            self.published_engine = self.engine.clone();
            return Ok(None);
        }
        let temporary = self.engine.temporary_mode().is_some()
            || self.state_machine().mode() == InputMode::Menu;
        let op = self.finish_operation(temporary);
        let mut sm = runtime.state_machine();
        sm.set_clock(Arc::clone(&self.clock));
        let mut next = SemanticEngine::new(sm, self.engine.profile);
        let _ = next.set_expression_context(
            self.context.purpose == InputPurpose::Text && self.context.allows_assistance,
        );
        if !self.legacy() && self.engine.base_mode() == BaseMode::English {
            next.dispatch(Action::SetBaseMode(BaseMode::English));
        }
        self.config = runtime.config.clone();
        self.resources = Arc::clone(&runtime);
        self.prediction_dict = Arc::clone(&runtime.dict);
        self.clear_prediction_context();
        self.prefix.bind(
            self.context.editor,
            self.context.epoch,
            (self.config.prediction.enabled || self.config.personal.predict)
                && self.context.purpose == InputPurpose::Text
                && self.context.allows_assistance,
        );
        self.revision = runtime.revision;
        // Runtime adoption is a control fact: cleanup failure never restores an older resource set.
        self.engine = next.clone();
        self.cleared(next, op, false).map(Some)
    }
    fn only_prediction_changed(&self, runtime: &RuntimeSnapshot) -> bool {
        if self.config.prediction == runtime.config.prediction
            && self.config.personal == runtime.config.personal
        {
            return false;
        }
        let mut previous = self.config.clone();
        previous.prediction = runtime.config.prediction.clone();
        previous.personal = runtime.config.personal.clone();
        let dictionaries_equal = |a: &Option<Arc<Dictionary>>, b: &Option<Arc<Dictionary>>| {
            a.as_ref().map(|d| (d.entries(), d.table_config()))
                == b.as_ref().map(|d| (d.entries(), d.table_config()))
        };
        serde_json::to_value(&previous).ok() == serde_json::to_value(&runtime.config).ok()
            && self.resources.dict.entries() == runtime.dict.entries()
            && self.resources.dict.table_config() == runtime.dict.table_config()
            && dictionaries_equal(&self.resources.temporary.pinyin, &runtime.temporary.pinyin)
            && dictionaries_equal(
                &self.resources.temporary.english,
                &runtime.temporary.english,
            )
            && self.resources.temporary.load_errors == runtime.temporary.load_errors
            && expression_resources_equal(&self.resources.expressions, &runtime.expressions)
    }
}

fn expression_resources_equal(
    previous: &crate::expressions::ExpressionResources,
    next: &crate::expressions::ExpressionResources,
) -> bool {
    crate::expressions::ExpressionKind::ALL
        .into_iter()
        .all(|kind| {
            let a = previous.resource(kind);
            let b = next.resource(kind);
            a.path == b.path
                && a.error == b.error
                && a.library.as_ref().map(|library| library.document())
                    == b.library.as_ref().map(|library| library.document())
        })
}
fn plan(
    sm: &StateMachine,
    show_hints: bool,
    transition: Transition,
    current: &SessionView,
) -> PlanParts {
    let mut view = SessionView {
        shortcut_error: sm.shortcut_error().map(str::to_owned),
        ..SessionView::default()
    };
    let mut unchanged = false;
    let mut consumed = true;
    let mut op = match transition {
        Transition::None => {
            unchanged = true;
            view = current.clone();
            EditSessionOp::NoOp
        }
        Transition::Commit(text) => EditSessionOp::CommitAndReplace {
            text,
            replace_len: 0,
        },
        Transition::Candidates {
            spelling,
            candidates,
            code_hints,
            page,
            total_pages,
        } => {
            if sm.mode() != InputMode::Main {
                set_candidates(
                    &mut view,
                    &spelling,
                    candidates,
                    code_hints,
                    page,
                    total_pages,
                    true,
                );
                if spelling.is_empty() {
                    EditSessionOp::EndComposition { delete_text: true }
                } else {
                    EditSessionOp::CompositionUpdate {
                        spelling,
                        attr: DisplayAttrKind::Input,
                    }
                }
            } else {
                if spelling.is_empty() {
                    EditSessionOp::EndComposition { delete_text: true }
                } else {
                    set_candidates(
                        &mut view,
                        &spelling,
                        candidates,
                        code_hints,
                        page,
                        total_pages,
                        show_hints,
                    );
                    EditSessionOp::CompositionUpdate {
                        spelling,
                        attr: DisplayAttrKind::Converted,
                    }
                }
            }
        }
        Transition::CommitThenCandidates {
            text,
            replace_len,
            spelling,
            candidates,
            code_hints,
            page,
            total_pages,
        } => {
            set_candidates(
                &mut view,
                &spelling,
                candidates,
                code_hints,
                page,
                total_pages,
                show_hints,
            );
            EditSessionOp::CommitThenUpdate {
                text,
                replace_len,
                spelling,
                attr: DisplayAttrKind::Converted,
            }
        }
        Transition::SpellingUpdated(spelling) => {
            view.visible = !spelling.is_empty();
            view.spelling = spelling.clone();
            if spelling.is_empty() {
                EditSessionOp::EndComposition { delete_text: true }
            } else {
                EditSessionOp::CompositionUpdate {
                    spelling,
                    attr: DisplayAttrKind::Input,
                }
            }
        }
        Transition::CommitThenSpelling {
            text,
            replace_len,
            spelling,
        } => {
            view.visible = !spelling.is_empty();
            view.spelling = spelling.clone();
            EditSessionOp::CommitThenUpdate {
                text,
                replace_len,
                spelling,
                attr: DisplayAttrKind::Input,
            }
        }
        Transition::Cleared => EditSessionOp::EndComposition { delete_text: true },
        Transition::Passthrough(_) => {
            unchanged = true;
            view = current.clone();
            consumed = false;
            EditSessionOp::NoOp
        }
    };
    if sm.mode() != InputMode::Main {
        view.visible = true;
        view.spelling = sm.spelling().to_owned();
    }
    view.mode = sm.mode();
    for (index, item) in view.items.iter_mut().enumerate() {
        if sm.is_shortcut_candidate(sm.candidate_offset() + index) {
            item.hint = "日期时间".into();
        }
    }
    view.expression = sm.expression_view();
    if view.expression.is_some()
        || (current.expression.is_some()
            && matches!(
                op,
                EditSessionOp::EndComposition { .. } | EditSessionOp::CompositionUpdate { .. }
            ))
    {
        // Query words and codes are local UI state. Never start, update or finish
        // host composition for this round, including cancellation and empty results.
        op = EditSessionOp::NoOp;
        view.preedit_kind = None;
    }
    view.preedit_kind = match &op {
        EditSessionOp::CompositionUpdate { attr, .. }
        | EditSessionOp::CommitThenUpdate { attr, .. } => Some(*attr),
        _ => view.preedit_kind,
    };
    PlanParts {
        reset_view: false,
        unchanged,
        op,
        view,
        consumed,
    }
}

fn set_candidates(
    view: &mut SessionView,
    spelling: &str,
    candidates: Vec<String>,
    hints: Vec<String>,
    page: usize,
    total_pages: usize,
    show_hints: bool,
) {
    view.visible = true;
    view.spelling = spelling.to_string();
    view.page = page;
    view.total_pages = total_pages;
    view.items = candidates
        .into_iter()
        .enumerate()
        .map(|(i, text)| PresentationItem {
            text,
            hint: if show_hints {
                hints.get(i).cloned().unwrap_or_default()
            } else {
                String::new()
            },
        })
        .collect();
}
