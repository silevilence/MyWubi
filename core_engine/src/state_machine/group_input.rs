use super::{semantic::*, *};
use crate::candidates::CandidateHint;
use crate::group_input::{BoundMapping, Position, Sequence, MAX_POSITIONS};

impl StateMachine {
    pub fn group_sequence(&self) -> Option<&Sequence> {
        self.groups.as_ref()
    }
    pub fn group_search_status(&self) -> Option<crate::group_search::SearchStatus> {
        self.group_matches.as_ref().map(|m| m.status)
    }
    pub fn group_shortcut_protected(&self) -> bool {
        self.shortcut_active()
            && self
                .groups
                .as_ref()
                .is_some_and(|s| self.shortcut_rules.reaches_groups(&s.positions))
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
        let previous_groups = self.sm.groups.clone();
        let result = self.change_groups(action);
        if result.is_ok()
            && previous_groups != self.sm.groups
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
                if !matches!(self.sm.mode, InputMode::Main | InputMode::Pinyin)
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
                if self
                    .sm
                    .groups
                    .as_ref()
                    .and_then(|s| s.positions.get(*index))
                    .is_some_and(|p| p.selected == Some(*character))
                {
                    return Ok(Transition::None);
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
                if !self
                    .sm
                    .groups
                    .as_mut()
                    .ok_or(Rejection::StaleCandidate)?
                    .undo()
                {
                    return Ok(Transition::None);
                }
            }
            Action::InputCode(c)
            | Action::InputText(c, TextIntent::Normal)
            | Action::Legacy(InputEvent::Char(c)) => {
                if !self.group_accepts(*c) {
                    return Err(Rejection::InvalidCode);
                }
                if let Some(sequence) = &self.sm.groups {
                    let mut next = sequence.positions.clone();
                    next.push(Position::exact(*c));
                    if sequence.resolved().is_some()
                        && self.sm.mode == InputMode::Main
                        && sequence.positions.len() >= self.sm.max_code_len
                        && self.sm.commit_on_max_code_overflow
                        && !self.sm.candidates.is_empty()
                        && self.sm.group_search_status()
                            == Some(crate::group_search::SearchStatus::Complete)
                        && !(self.sm.shortcut_active()
                            && self.sm.shortcut_rules.reaches_groups(&next))
                    {
                        let text = self.sm.candidates[0].clone();
                        let dynamic = self.sm.is_shortcut_candidate(self.sm.candidate_offset());
                        let transition = self.sm.commit_and_start_next(text, 0, *c);
                        self.sm.shortcut_committed = dynamic;
                        return Ok(transition);
                    }
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
                        let mode = self.sm.mode;
                        self.sm.reset();
                        self.sm.mode = mode;
                        return Ok(Transition::Cleared);
                    }
                }
            }
            Action::Cancel | Action::Legacy(InputEvent::Esc) => {
                self.end_temporary();
                return Ok(Transition::Cleared);
            }
            Action::SelectCandidate(query, id) => {
                if *query != self.candidate_window(0, 0).query_revision {
                    return Err(Rejection::StaleCandidate);
                }
                return self.commit_group_candidate(id.0);
            }
            Action::Space | Action::CommitPreferred | Action::Legacy(InputEvent::Space) => {
                return self.commit_group_candidate(self.sm.candidate_offset());
            }
            Action::Legacy(InputEvent::Select(index)) => {
                let slot = index
                    .checked_sub(1)
                    .filter(|i| *i < self.sm.candidate_page_capacity())
                    .ok_or(Rejection::StaleCandidate)?;
                return self.commit_group_candidate(self.sm.candidate_offset() + slot);
            }
            Action::Enter | Action::CommitRaw | Action::Legacy(InputEvent::Enter) => {
                let text = self
                    .sm
                    .groups
                    .as_ref()
                    .and_then(Sequence::resolved)
                    .ok_or_else(|| {
                        Rejection::Unavailable("请先明确字符组，再提交原始编码".into())
                    })?;
                self.end_temporary();
                return Ok(Transition::Commit(text));
            }
            Action::MoveCandidatePage(next) => {
                self.sm.page = if *next {
                    (self.sm.page + 1).min(self.sm.total_pages().saturating_sub(1))
                } else {
                    self.sm.page.saturating_sub(1)
                };
                return self.show_group_page();
            }
            Action::Legacy(InputEvent::PageNext | InputEvent::PagePrev) => {
                let next = matches!(action, Action::Legacy(InputEvent::PageNext));
                self.sm.page = if next {
                    (self.sm.page + 1).min(self.sm.total_pages().saturating_sub(1))
                } else {
                    self.sm.page.saturating_sub(1)
                };
                return self.show_group_page();
            }
            _ => {
                return Err(Rejection::Unavailable(
                    "请先选词、明确字符组或取消当前输入".into(),
                ))
            }
        }
        let transition = self.refresh_groups()?;
        if matches!(
            action,
            Action::InputCode(_)
                | Action::InputText(..)
                | Action::InputGroup(..)
                | Action::ResolveGroup(..)
                | Action::Legacy(InputEvent::Char(_))
        ) && self.sm.mode == InputMode::Main
            && self.sm.auto_commit_unique
            && self.sm.groups.as_ref().is_some_and(|s| {
                s.resolved().is_some() && s.positions.len() >= self.sm.max_code_len
            })
            && !self.sm.group_shortcut_protected()
            && self.sm.group_search_status() == Some(crate::group_search::SearchStatus::Complete)
            && self.sm.total_candidates == 1
        {
            return self.commit_group_candidate(0);
        }
        Ok(transition)
    }
    fn commit_group_candidate(&mut self, index: usize) -> Result<Transition, Rejection> {
        let candidate = self
            .candidate_window(index, 1)
            .candidates
            .into_iter()
            .next()
            .ok_or_else(|| {
                Rejection::Unavailable("当前没有可选候选，请继续输入或明确字母".into())
            })?;
        let text = candidate.text;
        let dynamic = candidate
            .hints
            .iter()
            .any(|h| matches!(h, CandidateHint::DateTime(_)));
        self.end_temporary();
        self.sm.shortcut_committed = dynamic;
        Ok(Transition::Commit(text))
    }
    fn show_group_page(&mut self) -> Result<Transition, Rejection> {
        if self.sm.appended.is_some() {
            self.sm.apply_appended_page();
        } else {
            let matches = self
                .sm
                .group_matches
                .as_ref()
                .ok_or(Rejection::StaleCandidate)?;
            let start = self.sm.candidate_offset();
            self.sm.candidates = matches
                .candidates
                .iter()
                .skip(start)
                .take(self.sm.page_size)
                .map(|m| m.text.clone())
                .collect();
            self.sm.code_hints = matches
                .candidates
                .iter()
                .skip(start)
                .take(self.sm.page_size)
                .map(|m| m.codes.join("/"))
                .collect();
        }
        self.sm.state = if self.sm.candidates.is_empty() {
            InputState::Composing
        } else {
            InputState::Selecting
        };
        Ok(Transition::Candidates {
            spelling: self.sm.spelling.clone(),
            candidates: self.sm.candidates.clone(),
            code_hints: self.sm.code_hints.clone(),
            page: self.sm.page,
            total_pages: self.sm.total_pages(),
        })
    }
    fn group_accepts(&self, c: char) -> bool {
        match self.sm.mode {
            InputMode::Main => {
                self.sm.dict.table_config().charset.contains(c)
                    || (self.sm.shortcut_active() && c.is_ascii_alphabetic())
            }
            InputMode::Pinyin => c.is_ascii_lowercase() || c == '\'',
            _ => false,
        }
    }
    fn refresh_groups(&mut self) -> Result<Transition, Rejection> {
        let sequence = self.sm.groups.as_ref().ok_or(Rejection::InvalidCode)?;
        self.sm.spelling = sequence.hint();
        self.sm.page = 0;
        let dict = if self.sm.mode == InputMode::Pinyin {
            self.sm
                .temporary
                .pinyin
                .as_ref()
                .ok_or_else(|| Rejection::Unavailable("未配置拼音码表".into()))?
        } else {
            &self.sm.dict
        };
        let matches =
            crate::group_search::query(dict, &sequence.positions, crate::group_search::WORK_BUDGET);
        let resolved = sequence.resolved().is_some();
        self.sm.total_candidates = matches.candidates.len();
        self.sm.group_matches = Some(Arc::new(matches));
        self.sm.appended = None;
        self.sm.shortcut_time = None;
        if resolved {
            self.sm.capture_shortcut_time();
        }
        if !self.sm.may_append_candidates() {
            return self.show_group_page();
        }
        let entries: Vec<_> = self
            .sm
            .group_matches
            .as_ref()
            .unwrap()
            .candidates
            .iter()
            .map(|m| Entry {
                word: m.text.clone(),
                code: m.codes.first().cloned().unwrap_or_default(),
                weight: m.weight,
            })
            .collect();
        self.sm.appended = self
            .sm
            .build_appended(&entries.iter().collect::<Vec<_>>())
            .map(Arc::new);
        self.show_group_page()
    }
}
