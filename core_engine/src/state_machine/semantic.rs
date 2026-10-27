//! Platform-independent character intent. Legacy events retain their exact routing.
use super::*;
use crate::candidates::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_ENGINE: AtomicU64 = AtomicU64::new(1);

/// Persistent selection; a temporary capability never overwrites this choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BaseMode {
    #[default]
    Main,
    English,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporaryMode {
    Pinyin,
    English,
}

/// Availability is independent of menu bindings and text completion availability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub available: bool,
    pub candidate_support: bool,
    pub reason: Option<String>,
}

/// Input policy, independent of the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorProfile {
    LegacyV1,
    TouchV1,
}

/// Normal text is classified by the engine; literal text explicitly bypasses lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextIntent {
    Normal,
    Literal,
}

/// Semantic actions do not contain platform key codes or display coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Legacy(InputEvent),
    InputCode(char),
    InputText(char, TextIntent),
    InputSymbol(char),
    InsertLiteral(String),
    CommitPreferred,
    CommitRaw,
    Space,
    Enter,
    Backspace,
    Cancel,
    MoveCandidatePage(bool),
    SetBaseMode(BaseMode),
    BeginTemporary(TemporaryMode),
    EndTemporary,
    OpenModeChooser,
    ChooseMode(TemporaryMode),
    CloseModeChooser,
    SelectCandidate(QueryRevision, CandidateId),
}

impl From<InputEvent> for Action {
    fn from(event: InputEvent) -> Self {
        Self::Legacy(event)
    }
}

/// A rejection never doubles as a platform key passthrough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    InvalidCode,
    TextTooLong,
    Unavailable(String),
    StaleCandidate,
}

/// Dispatch classification, kept separate from visible changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    Handled,
    NotHandled,
    Rejected(Rejection),
}

/// A transition may leave the visible view unchanged while consuming an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionResult {
    pub disposition: Disposition,
    pub transition: Transition,
}

impl ActionResult {
    pub(super) fn from_transition(transition: Transition) -> Self {
        let disposition = if matches!(transition, Transition::Passthrough(_)) {
            Disposition::NotHandled
        } else {
            Disposition::Handled
        };
        Self {
            disposition,
            transition,
        }
    }
    pub(super) fn rejected(reason: Rejection) -> Self {
        Self {
            disposition: Disposition::Rejected(reason),
            transition: Transition::None,
        }
    }
}

/// Owns one input buffer and shares immutable dictionaries. No I/O in dispatch/query.
#[derive(Clone)]
pub struct SemanticEngine {
    pub(super) sm: StateMachine,
    pub profile: BehaviorProfile,
    base: BaseMode,
    temporary: Option<TemporaryMode>,
    query_revision: QueryRevision,
    view_revision: u64,
    all_candidates: Arc<Vec<Candidate>>,
}

impl SemanticEngine {
    pub fn new(sm: StateMachine, profile: BehaviorProfile) -> Self {
        Self {
            sm,
            profile,
            base: BaseMode::Main,
            temporary: None,
            query_revision: QueryRevision {
                engine: NEXT_ENGINE.fetch_add(1, Ordering::Relaxed),
                version: 0,
            },
            view_revision: 0,
            all_candidates: Arc::new(Vec::new()),
        }
    }
    pub fn state_machine(&self) -> &StateMachine {
        &self.sm
    }

    pub fn candidate_window(&self, offset: usize, count: usize) -> CandidateWindow {
        let offset = offset.min(self.all_candidates.len());
        let end = offset
            .saturating_add(count.min(MAX_WINDOW))
            .min(self.all_candidates.len());
        CandidateWindow {
            query_revision: self.query_revision,
            view_revision: self.view_revision,
            total: self.all_candidates.len(),
            offset,
            candidates: self.all_candidates[offset..end].to_vec(),
        }
    }

    /// A context/runtime change invalidates even identical candidate texts.
    pub fn invalidate_query(&mut self) {
        self.query_revision.version += 1;
        self.view_revision += 1;
        self.refresh_candidates();
    }

    fn refresh_candidates(&mut self) {
        let dict = match self.sm.mode {
            InputMode::Main => Some(&self.sm.dict),
            InputMode::Pinyin => self.sm.temporary.pinyin.as_ref(),
            InputMode::English => self.sm.temporary.english.as_ref(),
            InputMode::Menu => None,
        };
        self.all_candidates = Arc::new(
            if let Some(dict) =
                dict.filter(|_| !self.sm.spelling.is_empty() && !self.sm.candidates.is_empty())
            {
                let options = SearchOptions {
                    prefer_exact: true,
                    limit: dict.len(),
                };
                let all = if self.sm.mode == InputMode::Main {
                    dict.search(self.sm.lookup_key(), options)
                } else {
                    dict.search_literal(&self.sm.spelling, options)
                };
                all.into_iter()
                    .enumerate()
                    .map(|(index, entry)| {
                        let hints = if self.sm.mode == InputMode::Main {
                            entry
                                .code
                                .strip_prefix(self.sm.lookup_key())
                                .filter(|s| !s.is_empty())
                                .map(|s| vec![CandidateHint::CodeSuffix(s.to_owned())])
                                .unwrap_or_default()
                        } else if self.sm.dict.table_config().temporary.reverse_lookup {
                            self.sm
                                .temporary
                                .reverse
                                .as_ref()
                                .map(|r| r.codes(&entry.word))
                                .filter(|codes| !codes.is_empty())
                                .map(|codes| vec![CandidateHint::ReverseCodes(codes.to_vec())])
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        Candidate {
                            id: CandidateId(index),
                            text: entry.word.clone(),
                            exact: entry.code == self.sm.spelling,
                            hints,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            },
        );
    }

    pub fn base_mode(&self) -> BaseMode {
        self.base
    }
    pub fn temporary_mode(&self) -> Option<TemporaryMode> {
        if self.profile == BehaviorProfile::LegacyV1 {
            match self.sm.mode {
                InputMode::Pinyin => Some(TemporaryMode::Pinyin),
                InputMode::English => Some(TemporaryMode::English),
                _ => None,
            }
        } else {
            self.temporary
        }
    }

    pub fn capability(&self, mode: TemporaryMode) -> Capability {
        let tables = &self.sm.temporary;
        match mode {
            TemporaryMode::Pinyin => Capability {
                available: tables.pinyin.is_some(),
                candidate_support: tables.pinyin.is_some(),
                reason: tables
                    .pinyin_error
                    .clone()
                    .or_else(|| tables.pinyin.is_none().then(|| "未配置拼音码表".into())),
            },
            TemporaryMode::English => Capability {
                available: self.profile == BehaviorProfile::TouchV1
                    || tables.english_error.is_none(),
                candidate_support: tables.english.is_some(),
                reason: tables.english_error.clone(),
            },
        }
    }

    /// End a round without changing the persistent mode; old StateMachine::reset stays unchanged.
    pub fn clear_composition(&mut self) {
        self.sm.reset();
        self.temporary = None;
        if self.base == BaseMode::English {
            self.sm.mode = InputMode::English;
        }
    }

    pub fn reset_context(&mut self) {
        self.clear_composition();
    }

    fn persistent_english(&self) -> bool {
        self.base == BaseMode::English && self.temporary.is_none()
    }

    fn change_mode(
        &mut self,
        base: Option<BaseMode>,
        temporary: Option<TemporaryMode>,
    ) -> ActionResult {
        if (base.is_some() && base == Some(self.base) && self.temporary.is_none())
            || (temporary.is_some() && self.temporary_mode() == temporary)
        {
            return ActionResult::from_transition(Transition::None);
        }
        let capability = temporary
            .or_else(|| (base == Some(BaseMode::English)).then_some(TemporaryMode::English));
        if let Some(mode) = capability {
            let capability = self.capability(mode);
            if !capability.available {
                return ActionResult::rejected(Rejection::Unavailable(
                    capability.reason.unwrap_or_default(),
                ));
            }
        }
        let raw = if self.temporary_mode().is_none() && self.sm.mode != InputMode::Menu {
            self.sm.spelling.clone()
        } else {
            String::new()
        };
        self.sm.reset();
        if let Some(base) = base {
            self.base = base;
        }
        self.temporary = temporary;
        self.sm.mode = match temporary {
            Some(TemporaryMode::Pinyin) => InputMode::Pinyin,
            Some(TemporaryMode::English) => InputMode::English,
            None if self.base == BaseMode::English => InputMode::English,
            None => InputMode::Main,
        };
        ActionResult::from_transition(if raw.is_empty() {
            Transition::Cleared
        } else {
            Transition::Commit(raw)
        })
    }

    /// Pure routing prediction: does not clone the state or execute a search.
    pub fn would_handle(&self, action: &Action) -> bool {
        match action {
            Action::SelectCandidate(revision, id) => {
                *revision == self.query_revision && id.0 < self.all_candidates.len()
            }
            Action::SetBaseMode(_)
            | Action::BeginTemporary(_)
            | Action::EndTemporary
            | Action::OpenModeChooser
            | Action::ChooseMode(_)
            | Action::CloseModeChooser => true,
            Action::Legacy(e) => self.sm.would_consume(e),
            Action::InputCode(c) => self.sm.accepts_code_char(*c),
            Action::InsertLiteral(s) => !s.is_empty() && s.len() <= 65536,
            Action::InputText(..) | Action::InputSymbol(_) => true,
            Action::CommitPreferred | Action::CommitRaw => !self.sm.spelling().is_empty(),
            Action::Space => self.sm.would_consume(&InputEvent::Space),
            Action::Enter | Action::Backspace if self.persistent_english() => {
                !self.sm.spelling.is_empty()
            }
            Action::Enter => self.sm.would_consume(&InputEvent::Enter),
            Action::Backspace => self.sm.would_consume(&InputEvent::Backspace),
            Action::Cancel => self.sm.would_consume(&InputEvent::Esc),
            Action::MoveCandidatePage(_) => true,
        }
    }

    pub fn dispatch(&mut self, action: Action) -> ActionResult {
        let before = (
            self.sm.spelling.clone(),
            self.sm.mode,
            self.base,
            self.temporary,
        );
        let page = self.sm.page;
        let result = self.dispatch_inner(action);
        let changed = before
            != (
                self.sm.spelling.clone(),
                self.sm.mode,
                self.base,
                self.temporary,
            );
        if changed
            || matches!(
                result.transition,
                Transition::Commit(_)
                    | Transition::Cleared
                    | Transition::CommitThenCandidates { .. }
                    | Transition::CommitThenSpelling { .. }
            )
        {
            self.invalidate_query();
        } else if page != self.sm.page {
            self.view_revision += 1;
        }
        result
    }

    fn dispatch_inner(&mut self, action: Action) -> ActionResult {
        let legacy = matches!(action, Action::Legacy(_));
        let transition = match action {
            Action::SelectCandidate(revision, id) => {
                if revision != self.query_revision {
                    return ActionResult::rejected(Rejection::StaleCandidate);
                }
                let Some(candidate) = self.all_candidates.get(id.0) else {
                    return ActionResult::rejected(Rejection::StaleCandidate);
                };
                let text = candidate.text.clone();
                self.clear_composition();
                Transition::Commit(text)
            }
            Action::SetBaseMode(base) => return self.change_mode(Some(base), None),
            Action::BeginTemporary(mode) | Action::ChooseMode(mode) => {
                return self.change_mode(None, Some(mode))
            }
            Action::EndTemporary | Action::CloseModeChooser => {
                self.clear_composition();
                Transition::Cleared
            }
            Action::OpenModeChooser => {
                if !self.sm.spelling.is_empty() {
                    return ActionResult::rejected(Rejection::Unavailable("输入缓冲非空".into()));
                }
                self.sm.mode = InputMode::Menu;
                self.sm.refresh_temporary()
            }
            Action::Legacy(event) => self.sm.handle(event),
            Action::InputCode(c) if self.sm.mode != InputMode::Main => {
                self.sm.handle(InputEvent::Char(c))
            }
            Action::InputCode(c) => {
                if !self.sm.accepts_code_char(c) {
                    return ActionResult::rejected(Rejection::InvalidCode);
                }
                self.sm.on_code_char(c)
            }
            Action::InputText(c, TextIntent::Normal)
                if self.sm.mode == InputMode::Main && self.sm.accepts_code_char(c) =>
            {
                self.sm.on_code_char(c)
            }
            Action::InputText(c, TextIntent::Normal)
                if self.persistent_english() && !c.is_alphabetic() =>
            {
                self.insert_literal(&c.to_string())
            }
            Action::InputText(c, TextIntent::Normal)
                if self.sm.mode != InputMode::Main && c.is_ascii_digit() =>
            {
                self.insert_literal(&c.to_string())
            }
            Action::InputText(c, TextIntent::Normal) if self.sm.mode != InputMode::Main => {
                self.sm.handle(InputEvent::Char(c))
            }
            Action::InputText(c, _) | Action::InputSymbol(c) => self.insert_literal(&c.to_string()),
            Action::InsertLiteral(text) => {
                if text.len() > 65536 {
                    return ActionResult::rejected(Rejection::TextTooLong);
                }
                self.insert_literal(&text)
            }
            Action::CommitPreferred => self
                .sm
                .commit_current()
                .map_or(Transition::None, |(text, _)| Transition::Commit(text)),
            Action::CommitRaw => self
                .sm
                .commit_raw_current()
                .map_or(Transition::None, |(text, _)| Transition::Commit(text)),
            Action::Space if self.persistent_english() => {
                let mut text = self
                    .sm
                    .commit_current()
                    .map_or_else(String::new, |(text, _)| text);
                text.push(' ');
                Transition::Commit(text)
            }
            Action::Enter if self.persistent_english() && self.sm.spelling.is_empty() => {
                Transition::Passthrough(InputEvent::Enter)
            }
            Action::Backspace if self.persistent_english() && self.sm.spelling.is_empty() => {
                Transition::Passthrough(InputEvent::Backspace)
            }
            Action::Space => self.sm.handle(InputEvent::Space),
            Action::Enter => self.sm.handle(InputEvent::Enter),
            Action::Backspace => self.sm.handle(InputEvent::Backspace),
            Action::Cancel => self.sm.handle(InputEvent::Esc),
            Action::MoveCandidatePage(next) => self.sm.handle(if next {
                InputEvent::PageNext
            } else {
                InputEvent::PagePrev
            }),
        };
        if !legacy && self.sm.mode == InputMode::Main && self.profile == BehaviorProfile::TouchV1 {
            self.temporary = None;
            if self.base == BaseMode::English {
                self.sm.mode = InputMode::English;
            }
        }
        ActionResult::from_transition(transition)
    }

    fn insert_literal(&mut self, literal: &str) -> Transition {
        if literal.is_empty() {
            return Transition::None;
        }
        let previous = if self.sm.mode == InputMode::Main {
            self.sm.commit_current()
        } else {
            self.sm.commit_raw_current()
        };
        let mut text = previous.map_or_else(String::new, |(text, _)| text);
        text.push_str(literal);
        Transition::Commit(text)
    }
}
