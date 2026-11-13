use super::*;
use crate::{
    shortcut_time::{Clock, LocalTime},
    shortcuts::{CompiledRule, RuleSet, ShortcutRule},
};

#[derive(Clone)]
pub(super) struct FrozenTime {
    code: String,
    value: Result<LocalTime, String>,
}

impl StateMachine {
    /// Zero-based page and slot in the same sequence used by keyboard selection.
    pub fn candidate_location(&self, index: usize) -> Option<(usize, usize)> {
        if index >= self.total_candidates {
            return None;
        }
        if let Some(view) = &self.appended {
            view.pages.iter().enumerate().find_map(|(page, range)| {
                range
                    .contains(&index)
                    .then_some((page, index.saturating_sub(range.start)))
            })
        } else {
            Some((index / self.page_size, index % self.page_size))
        }
    }
    pub fn committed_shortcut(&self) -> bool {
        self.shortcut_committed
    }
    pub(crate) fn is_shortcut_candidate(&self, index: usize) -> bool {
        self.appended_candidate(index).is_some_and(|candidate| {
            candidate
                .hints
                .iter()
                .any(|hint| matches!(hint, crate::candidates::CandidateHint::DateTime(_)))
        })
    }
    pub fn set_shortcut_rules(
        &mut self,
        rules: &[ShortcutRule],
    ) -> Result<(), crate::shortcuts::RuleError> {
        let rules = RuleSet::new(rules, self.dict.table_config().into())?;
        self.reset();
        self.shortcut_rules = Arc::new(rules);
        Ok(())
    }
    pub fn set_clock(&mut self, clock: Arc<dyn Clock>) {
        self.clock = clock;
        self.shortcut_time = None;
    }
    pub fn shortcut_error(&self) -> Option<&str> {
        self.shortcut_time
            .as_ref()
            .and_then(|snapshot| snapshot.value.as_ref().err().map(String::as_str))
    }
    pub fn shortcut_snapshot(&self) -> Option<&LocalTime> {
        self.shortcut_time.as_ref()?.value.as_ref().ok()
    }
    pub(super) fn shortcut_active(&self) -> bool {
        self.expressions_allowed && self.mode == InputMode::Main && !self.shortcut_rules.is_empty()
    }
    pub(super) fn shortcut_reaches(&self, code: &str) -> bool {
        self.shortcut_active() && self.shortcut_rules.reaches(code)
    }
    pub(super) fn capture_shortcut_time(&mut self) {
        if !self.shortcut_active() || self.shortcut_rules.exact(&self.spelling).is_empty() {
            self.shortcut_time = None;
            return;
        }
        let code = self.spelling.to_ascii_lowercase();
        if self
            .shortcut_time
            .as_ref()
            .is_some_and(|time| time.code == code)
        {
            return;
        }
        let value = self.clock.now().and_then(|time| {
            time.validate()?;
            Ok(time)
        });
        self.shortcut_time = Some(Arc::new(FrozenTime { code, value }));
    }
    pub(super) fn shortcut_outputs(&self) -> Vec<(&CompiledRule, String)> {
        if !self.shortcut_active() {
            return Vec::new();
        }
        let Some(time) = self.shortcut_snapshot() else {
            return Vec::new();
        };
        self.shortcut_rules
            .exact(&self.spelling)
            .iter()
            .filter_map(|rule| {
                crate::shortcut_time::render(&rule.parts, time)
                    .ok()
                    .map(|text| (rule, text))
            })
            .collect()
    }
}
