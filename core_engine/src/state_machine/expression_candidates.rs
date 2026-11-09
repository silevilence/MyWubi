//! One immutable candidate order for keyboard pages, touch windows and selection IDs.
use super::*;
use crate::candidates::{Candidate, CandidateHint, CandidateId};
use crate::expressions::{ExpressionKind, ExpressionLibrary};
use std::{collections::HashSet, ops::Range};

enum Row {
    Text(Entry),
    Expression(ExpressionKind, usize),
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
    pub(super) fn expression_library(&self, kind: ExpressionKind) -> Option<&ExpressionLibrary> {
        match kind {
            ExpressionKind::Emoji => self.expressions.emoji.library.as_deref(),
            ExpressionKind::Kaomoji => self.expressions.kaomoji.library.as_deref(),
        }
    }

    pub(super) fn build_appended(&self, text: &[&Entry]) -> Option<AppendedCandidates> {
        if !self.expressions_allowed
            || !(self.expression_config.emoji.append || self.expression_config.kaomoji.append)
            || !matches!(self.mode, InputMode::Main | InputMode::Pinyin)
            || text.is_empty()
        {
            return None;
        }
        let libraries = [
            (ExpressionKind::Emoji, self.expression_config.emoji.append),
            (
                ExpressionKind::Kaomoji,
                self.expression_config.kaomoji.append,
            ),
        ]
        .into_iter()
        .filter(|(_, enabled)| *enabled)
        .filter_map(|(kind, _)| self.expression_library(kind).map(|library| (kind, library)))
        .collect::<Vec<_>>();
        if libraries.is_empty() {
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
        has_expressions.then(|| AppendedCandidates {
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
                let hints = if self.mode == InputMode::Main {
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
                (entry.word.clone(), entry.code == self.spelling, hints)
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
        };
        Some(Candidate {
            expression_kind: match row {
                Row::Expression(kind, _) => Some(*kind),
                Row::Text(_) => None,
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
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
    }
}
