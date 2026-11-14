use super::{semantic::*, *};
use crate::group_input::{BoundMapping, Position, Sequence, MAX_POSITIONS};

impl StateMachine {
    pub fn group_sequence(&self) -> Option<&Sequence> {
        self.groups.as_ref()
    }
    pub fn group_search_status(&self) -> Option<crate::group_search::SearchStatus> {
        self.group_matches.as_ref().map(|m| m.status)
    }
}

impl SemanticEngine {
    pub(super) fn dispatch_groups(&mut self, action: &Action) -> Option<ActionResult> {
        if let Action::BindMapping(identity, definition) = action {
            let result = if !self.sm.spelling.is_empty() {
                ActionResult::rejected(Rejection::Unavailable("请结束当前输入后切换映射".into()))
            } else if !crate::skin::valid_id(&identity.id)
                || !crate::skin::valid_id(&identity.version)
                || identity.revision == 0
            {
                ActionResult::rejected(Rejection::InvalidCode)
            } else if let Err(reason) = definition.validate() {
                ActionResult::rejected(Rejection::Unavailable(reason))
            } else {
                self.mapping = Some(Arc::new(BoundMapping {
                    identity: identity.clone(),
                    definition: definition.clone(),
                }));
                self.invalidate_query();
                ActionResult::from_transition(Transition::None)
            };
            return Some(result);
        }
        let active = self.sm.groups.is_some();
        if !active
            && !matches!(
                action,
                Action::InputGroup(..) | Action::ResolveGroup(..) | Action::UndoGroupResolution(..)
            )
        {
            return None;
        }
        let result = self.change_groups(action);
        if result.is_ok()
            && matches!(
                action,
                Action::ResolveGroup(..) | Action::UndoGroupResolution(..)
            )
        {
            self.invalidate_query();
        }
        Some(match result {
            Ok(transition) => ActionResult::from_transition(transition),
            Err(reason) => ActionResult::rejected(reason),
        })
    }

    fn change_groups(&mut self, action: &Action) -> Result<Transition, Rejection> {
        match action {
            Action::InputGroup(identity, id) => {
                if self.sm.mode != InputMode::Main
                    || self.sm.expression_query.is_some()
                {
                    return Err(Rejection::Unavailable("当前模式不支持字符组".into()));
                }
                let mapping = self
                    .mapping
                    .as_ref()
                    .filter(|m| &m.identity == identity)
                    .ok_or(Rejection::StaleCandidate)?;
                let chars = mapping
                    .definition
                    .groups
                    .get(id)
                    .ok_or(Rejection::InvalidCode)?;
                if !chars.chars().all(|c| self.group_accepts(c)) {
                    return Err(Rejection::InvalidCode);
                }
                if self.sm.groups.is_none()
                    && !self.sm.spelling.chars().all(|c| self.group_accepts(c))
                {
                    return Err(Rejection::InvalidCode);
                }
                let mut sequence = self.sm.groups.clone().unwrap_or_else(|| Sequence {
                    mapping: identity.clone(),
                    positions: self.sm.spelling.chars().map(Position::exact).collect(),
                    resolutions: Vec::new(),
                });
                if sequence.positions.len() >= MAX_POSITIONS {
                    return Err(Rejection::TextTooLong);
                }
                sequence.positions.push(Position {
                    characters: chars.clone(),
                    selected: None,
                });
                self.sm.groups = Some(sequence);
            }
            Action::ResolveGroup(query, index, character) => {
                if *query != self.candidate_window(0, 0).query_revision {
                    return Err(Rejection::StaleCandidate);
                }
                self.sm
                    .groups
                    .as_mut()
                    .ok_or(Rejection::StaleCandidate)?
                    .resolve(*index, *character)
                    .map_err(Rejection::Unavailable)?;
            }
            Action::UndoGroupResolution(query) => {
                if *query != self.candidate_window(0, 0).query_revision {
                    return Err(Rejection::StaleCandidate);
                }
                self.sm
                    .groups
                    .as_mut()
                    .ok_or(Rejection::StaleCandidate)?
                    .undo();
            }
            Action::InputCode(c)
            | Action::InputText(c, TextIntent::Normal)
            | Action::Legacy(InputEvent::Char(c)) => {
                if !self.group_accepts(*c) {
                    return Err(Rejection::InvalidCode);
                }
                let sequence = self.sm.groups.as_mut().ok_or(Rejection::InvalidCode)?;
                if sequence.positions.len() >= MAX_POSITIONS {
                    return Err(Rejection::TextTooLong);
                }
                sequence.positions.push(Position::exact(*c));
            }
            Action::Backspace | Action::Legacy(InputEvent::Backspace) => {
                if let Some(sequence) = &mut self.sm.groups {
                    if !sequence.undo() {
                        sequence.positions.pop();
                    }
                    if sequence.positions.is_empty() {
                        self.sm.reset();
                        return Ok(Transition::Cleared);
                    }
                }
            }
            Action::Cancel | Action::Legacy(InputEvent::Esc) => {
                self.sm.reset();
                return Ok(Transition::Cleared);
            }
            _ => {
                return Err(Rejection::Unavailable(
                    "请先选词、明确字符组或取消当前输入".into(),
                ))
            }
        }
        self.refresh_groups()
    }
    fn group_accepts(&self, c: char) -> bool {
        match self.sm.mode {
            InputMode::Main => self.sm.dict.table_config().charset.contains(c),
            InputMode::Pinyin => c.is_ascii_lowercase() || c == '\'',
            _ => false,
        }
    }
    fn refresh_groups(&mut self) -> Result<Transition, Rejection> {
        let sequence = self.sm.groups.as_ref().ok_or(Rejection::InvalidCode)?;
        self.sm.spelling = sequence.hint();
        self.sm.page = 0;
        let matches = crate::group_search::query(
            &self.sm.dict,
            &sequence.positions,
            crate::group_search::WORK_BUDGET,
        );
        self.sm.total_candidates = matches.candidates.len();
        self.sm.candidates = matches
            .candidates
            .iter()
            .take(self.sm.page_size)
            .map(|m| m.text.clone())
            .collect();
        self.sm.code_hints = matches
            .candidates
            .iter()
            .take(self.sm.page_size)
            .map(|m| m.codes.join("/"))
            .collect();
        self.sm.group_matches = Some(Arc::new(matches));
        self.sm.appended = None;
        self.sm.shortcut_time = None;
        self.sm.state = InputState::Composing;
        Ok(Transition::Candidates {
            spelling: self.sm.spelling.clone(),
            candidates: self.sm.candidates.clone(),
            code_hints: self.sm.code_hints.clone(),
            page: 0,
            total_pages: self.sm.total_pages(),
        })
    }
}
