//! Two-phase host editing. Legacy processing advances before callbacks; touch waits for ack.
use crate::{candidates::*, runtime::RuntimeSnapshot, *};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

pub use crate::candidates::PreeditKind as DisplayAttrKind;

/// One operation remains one synchronous TSF edit session, including overflow.
#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTarget {
    OwnedComposition,
    CurrentSelection,
    LegacyRangeFallback,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputPurpose {
    Text,
    Direct,
    Password,
}

/// Platform supplies stable editor identity and explicit capabilities, not OS enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostContext {
    pub editor: u64,
    pub epoch: u64,
    pub supports_preedit: bool,
    pub supports_insert: bool,
    pub owns_preedit: bool,
    pub purpose: InputPurpose,
}
impl Default for HostContext {
    fn default() -> Self {
        Self {
            editor: 0,
            epoch: 0,
            supports_preedit: true,
            supports_insert: true,
            owns_preedit: false,
            purpose: InputPurpose::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditToken {
    pub session: u64,
    pub generation: u64,
    pub editor: u64,
    pub epoch: u64,
    pub runtime: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedEdit {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Ready,
    AwaitingAck,
    NeedsCleanup,
    Closed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    Busy,
    Closed,
    NeedsCleanup,
    Rejected(Rejection),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// A caller serializes access, releases its lock for host calls, then acknowledges.
/// Legacy permits nested prepares; only the newest generation may publish.
pub struct Session {
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
        let engine = SemanticEngine::new(runtime.state_machine(), profile);
        Self {
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
        self.published_engine.candidate_window(offset, count)
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
    pub fn would_handle(&self, action: &Action) -> bool {
        !self.closed && self.cleanup.is_none() && self.gate && self.engine.would_handle(action)
    }
    pub fn would_handle_legacy(&self, event: &InputEvent, system_modifier: bool) -> bool {
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
        if self.closed {
            return Err(SessionError::Closed);
        }
        if self.cleanup.is_some() {
            return Err(SessionError::NeedsCleanup);
        }
        if !self.legacy() && self.pending.is_some() {
            return Err(SessionError::Busy);
        }
        let mut next = self.engine.clone();
        let rollback = (self.legacy()
            && matches!(action, Action::Legacy(_))
            && self.state_machine().mode() != InputMode::Main)
            .then(|| self.engine.clone());
        let action = if !self.legacy() && self.context.purpose != InputPurpose::Text {
            match action {
                Action::InputText(c, _) | Action::InputCode(c) | Action::InputSymbol(c) => {
                    Action::InsertLiteral(c.to_string())
                }
                other => other,
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
        let mut parts = plan(
            next.state_machine(),
            self.config.basic.show_code_hints,
            result.transition,
            &self.view,
        );
        parts.consumed = result.disposition == Disposition::Handled;
        self.stage(next, rollback, parts, false)
    }

    fn stage(
        &mut self,
        next: SemanticEngine,
        rollback: Option<SemanticEngine>,
        mut parts: PlanParts,
        cleanup: bool,
    ) -> Result<PreparedEdit, SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        let target = if self.legacy() {
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
            token: self.token(),
            op: parts.op,
            target,
            preview: if parts.unchanged {
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
            plan: plan.clone(),
            next,
            rollback,
            cleanup,
        });
        Ok(plan)
    }

    pub fn acknowledge(&mut self, token: EditToken, outcome: HostOutcome) -> Acknowledged {
        if !self.is_current(token) {
            return Acknowledged {
                status: AckStatus::Stale,
                follow_up: None,
            };
        }
        let Some(pending) = self.pending.take() else {
            return Acknowledged {
                status: AckStatus::Stale,
                follow_up: None,
            };
        };
        self.last_outcome = Some(outcome);
        let status = match outcome {
            HostOutcome::Applied => {
                self.engine = pending.next;
                self.published_engine = self.engine.clone();
                if let ViewUpdate::Replace(view) = pending.plan.preview {
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
                AckStatus::Published
            }
            HostOutcome::RejectedBeforeWrite
                if !pending.cleanup && (!self.legacy() || pending.rollback.is_some()) =>
            {
                if let Some(previous) = pending.rollback {
                    self.engine = previous;
                }
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
                self.cleanup = (self.legacy() || self.context.owns_preedit).then_some(delete);
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
        if enabled {
            return Ok(None);
        }
        let mut next = self.engine.clone();
        let temporary = next.state_machine().mode() != InputMode::Main;
        let op = if temporary {
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
        let temporary = self.engine.temporary_mode().is_some()
            || self.state_machine().mode() == InputMode::Menu;
        let mut next = self.engine.clone();
        next.reset_context();
        next.invalidate_query();
        self.cleared(
            next,
            EditSessionOp::EndComposition {
                delete_text: temporary,
            },
            false,
        )
    }
    /// A dead connection/terminated composition cannot be cleaned through the new selection.
    pub fn invalidate_context(&mut self, context: HostContext, preserve_base: bool) {
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
        self.published_engine = self.engine.clone();
        self.view = SessionView::default();
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
        if !self.legacy() && self.pending.is_some() {
            self.deferred_runtime = Some(runtime);
            return Ok(None);
        }
        let temporary = self.engine.temporary_mode().is_some()
            || self.state_machine().mode() == InputMode::Menu;
        let mut next = SemanticEngine::new(runtime.state_machine(), self.engine.profile);
        if !self.legacy() && self.engine.base_mode() == BaseMode::English {
            next.dispatch(Action::SetBaseMode(BaseMode::English));
        }
        self.config = runtime.config.clone();
        self.revision = runtime.revision;
        // Runtime adoption is a control fact: cleanup failure never restores an older resource set.
        self.engine = next.clone();
        self.cleared(
            next,
            EditSessionOp::EndComposition {
                delete_text: temporary,
            },
            false,
        )
        .map(Some)
    }
}
fn plan(
    sm: &StateMachine,
    show_hints: bool,
    transition: Transition,
    current: &SessionView,
) -> PlanParts {
    let mut view = SessionView::default();
    let mut unchanged = false;
    let mut consumed = true;
    let op = match transition {
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
    view.preedit_kind = match &op {
        EditSessionOp::CompositionUpdate { attr, .. }
        | EditSessionOp::CommitThenUpdate { attr, .. } => Some(*attr),
        _ => view.preedit_kind,
    };
    PlanParts {
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
