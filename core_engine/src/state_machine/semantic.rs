//! Platform-independent character intent. Legacy events retain their exact routing.
use super::*;

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
}

impl SemanticEngine {
    pub fn new(sm: StateMachine, profile: BehaviorProfile) -> Self {
        Self { sm, profile }
    }
    pub fn state_machine(&self) -> &StateMachine {
        &self.sm
    }

    /// Pure routing prediction: does not clone the state or execute a search.
    pub fn would_handle(&self, action: &Action) -> bool {
        match action {
            Action::Legacy(e) => self.sm.would_consume(e),
            Action::InputCode(c) => self.sm.accepts_code_char(*c),
            Action::InsertLiteral(s) => !s.is_empty() && s.len() <= 65536,
            Action::InputText(..) | Action::InputSymbol(_) => true,
            Action::CommitPreferred | Action::CommitRaw => !self.sm.spelling().is_empty(),
            Action::Space => self.sm.would_consume(&InputEvent::Space),
            Action::Enter => self.sm.would_consume(&InputEvent::Enter),
            Action::Backspace => self.sm.would_consume(&InputEvent::Backspace),
            Action::Cancel => self.sm.would_consume(&InputEvent::Esc),
            Action::MoveCandidatePage(_) => true,
        }
    }

    pub fn dispatch(&mut self, action: Action) -> ActionResult {
        let transition = match action {
            Action::Legacy(event) => self.sm.handle(event),
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
