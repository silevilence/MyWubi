//! One immutable candidate order for keyboard pages, touch windows and selection IDs.
use super::*;
use crate::candidates::{Candidate, CandidateHint, CandidateId};
use crate::expressions::{ExpressionKind, ExpressionLibrary};
use std::{collections::HashSet, ops::Range};

enum Row {
    Text(Entry),
    Expression(ExpressionKind, usize),
    Shortcut { rule_index: usize, text: String },
}

/// Text pages retain their original grouping. Expressions fill any free slots, then
/// continue on dedicated pages before the next text page starts. Short continuation
/// pages are not filled with the next text group, preserving that group's first choice.
pub(super) struct AppendedCandidates {
    spelling: String,
    mode: InputMode,
    rows: Vec<Row>,
    pub(super) pages: Vec<Range<usize>>,
}

impl StateMachine {
    pub(super) fn may_append_candidates(&self) -> bool {
        self.shortcut_time.is_some()
            || (self.expressions_allowed
                && matches!(self.mode, InputMode::Main | InputMode::Pinyin)
                && ExpressionKind::ALL.into_iter().any(|kind| {
                    self.expression_config.selection(kind).append
                        && self.expression_library(kind).is_some()
                }))
    }
    pub(super) fn has_shortcut_candidates(&self) -> bool {
        self.appended.as_ref().is_some_and(|view| {
            view.rows
                .iter()
                .any(|row| matches!(row, Row::Shortcut { .. }))
        })
    }
    pub(super) fn expression_library(&self, kind: ExpressionKind) -> Option<&ExpressionLibrary> {
        self.expressions.resource(kind).library.as_deref()
    }

    pub(super) fn build_appended(&self, text: &[&Entry]) -> Option<AppendedCandidates> {
        let dynamic = self.shortcut_outputs();
        let libraries = ExpressionKind::ALL
            .into_iter()
            .filter(|kind| {
                self.expressions_allowed
                    && matches!(self.mode, InputMode::Main | InputMode::Pinyin)
                    && self.expression_config.selection(*kind).append
            })
            .filter_map(|kind| self.expression_library(kind).map(|library| (kind, library)))
            .collect::<Vec<_>>();
        if libraries.is_empty() && dynamic.is_empty() {
            return None;
        }
        let mut rows = Vec::with_capacity(text.len());
        let mut pages = Vec::new();
        // Existing text always wins a duplicate output; expressions never trigger more lookup.
        let mut seen: HashSet<&str> = text.iter().map(|entry| entry.word.as_str()).collect();
        let mut has_expressions = false;
        for group in text.chunks(self.page_size) {
            let start = rows.len();
            rows.extend(group.iter().map(|entry| Row::Text((*entry).clone())));
            // Earlier text candidate first; within each trigger Emoji precedes kaomoji,
            // with library order preserved. Deduplicate over the whole query, not one page.
            for entry in group {
                for &(kind, library) in &libraries {
                    for &index in library.exact_indices(&entry.word) {
                        let expression = &library.document().entries[index];
                        if seen.insert(&expression.text) {
                            rows.push(Row::Expression(kind, index));
                            has_expressions = true;
                        }
                    }
                }
            }
            // Legacy digits select 1..=9; do not introduce an unreachable tenth item.
            let capacity = self.page_size.min(9);
            for offset in (start..rows.len()).step_by(capacity) {
                pages.push(offset..(offset + capacity).min(rows.len()));
            }
        }
        let has_dynamic = !dynamic.is_empty();
        if has_dynamic {
            rows.retain(|row| match row {
                Row::Expression(kind, index) => {
                    self.expression_library(*kind).is_some_and(|library| {
                        !dynamic
                            .iter()
                            .any(|(_, text)| *text == library.document().entries[*index].text)
                    })
                }
                _ => true,
            });
            for (rule, text) in dynamic {
                let position = (rule.position as usize - 1).min(rows.len());
                rows.insert(
                    position,
                    Row::Shortcut {
                        rule_index: rule.rule_index,
                        text,
                    },
                );
            }
            let capacity = self.page_size.min(9);
            pages = (0..rows.len())
                .step_by(capacity)
                .map(|start| start..(start + capacity).min(rows.len()))
                .collect();
        }
        (has_expressions || has_dynamic).then(|| AppendedCandidates {
            spelling: self.spelling.clone(),
            mode: self.mode,
            rows,
            pages,
        })
    }

    pub(super) fn reuse_appended_page(&mut self) -> bool {
        if self
            .appended
            .as_ref()
            .is_some_and(|view| view.spelling == self.spelling && view.mode == self.mode)
        {
            self.apply_appended_page();
            true
        } else {
            self.appended = None;
            false
        }
    }

    pub(super) fn appended_candidate(&self, index: usize) -> Option<Candidate> {
        let row = self.appended.as_ref()?.rows.get(index)?;
        let (text, exact, hints) = match row {
            Row::Text(entry) => {
                let group = self
                    .group_matches
                    .as_ref()
                    .and_then(|m| m.candidates.iter().find(|m| m.text == entry.word));
                let hints = if let Some(m) = group {
                    vec![CandidateHint::ReverseCodes(m.codes.clone())]
                } else if self.mode == InputMode::Main {
                    entry
                        .code
                        .strip_prefix(self.lookup_key())
                        .filter(|suffix| !suffix.is_empty())
                        .map(|suffix| vec![CandidateHint::CodeSuffix(suffix.into())])
                        .unwrap_or_default()
                } else if self.dict.table_config().temporary.reverse_lookup {
                    self.temporary
                        .reverse
                        .as_ref()
                        .map(|index| index.codes(&entry.word))
                        .filter(|codes| !codes.is_empty())
                        .map(|codes| vec![CandidateHint::ReverseCodes(codes.to_vec())])
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                (
                    entry.word.clone(),
                    group.map_or(entry.code == self.spelling, |m| m.exact),
                    hints,
                )
            }
            Row::Expression(kind, index) => {
                let entry = self
                    .expression_library(*kind)?
                    .document()
                    .entries
                    .get(*index)?;
                (
                    entry.text.clone(),
                    true,
                    vec![CandidateHint::ExpressionName(entry.name.clone())],
                )
            }
            Row::Shortcut { rule_index, text } => (
                text.clone(),
                true,
                vec![CandidateHint::DateTime(*rule_index)],
            ),
        };
        Some(Candidate {
            expression_kind: match row {
                Row::Expression(kind, _) => Some(*kind),
                Row::Text(_) | Row::Shortcut { .. } => None,
            },
            id: CandidateId(index),
            text,
            exact,
            hints,
        })
    }

    pub(super) fn apply_appended_page(&mut self) {
        let Some(view) = &self.appended else {
            return;
        };
        self.total_candidates = view.rows.len();
        self.page = self.page.min(view.pages.len().saturating_sub(1));
        let range = view.pages.get(self.page).cloned().unwrap_or(0..0);
        let candidates: Vec<_> = range
            .filter_map(|index| self.appended_candidate(index))
            .collect();
        self.candidates = candidates
            .iter()
            .map(|candidate| candidate.text.clone())
            .collect();
        self.code_hints = candidates
            .into_iter()
            .map(|candidate| {
                candidate
                    .hints
                    .into_iter()
                    .map(|hint| match hint {
                        CandidateHint::CodeSuffix(text)
                        | CandidateHint::ExpressionName(text)
                        | CandidateHint::ExpressionMatch(text) => text,
                        CandidateHint::ReverseCodes(codes) => codes.join(" / "),
                        CandidateHint::DateTime(_) => "日期时间".into(),
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
    }
}
