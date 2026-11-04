//! Bounded, deterministic word recall. No generated sentences or automatic commits.
use crate::{candidates::*, Entry};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_VIEW: AtomicU64 = AtomicU64::new(1);

/// A small authored association lexicon, not a trained language model. All
/// suggestions are individual words; source and license live beside this file.
const ASSOCIATIONS: &str = include_str!("prediction_pairs.tsv");

#[derive(Debug)]
pub struct Lexicon {
    words: Vec<(String, u32)>,
}
impl Lexicon {
    pub fn new(entries: &[Entry]) -> Self {
        let mut words = BTreeMap::<String, u32>::new();
        for line in include_str!("prediction_words.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            if let Some((text, weight)) = line.split_once('\t') {
                if let Ok(weight) = weight.parse::<u32>() {
                    words.insert(text.to_owned(), weight);
                }
            }
        }
        for entry in entries.iter().filter(|e| word(&e.word)) {
            words
                .entry(entry.word.clone())
                .and_modify(|n| *n = (*n).max(entry.weight))
                .or_insert(entry.weight);
        }
        Self {
            words: words.into_iter().collect(),
        }
    }
    /// Lookup is bounded to eight suffixes and at most 1024 rows each. The index
    /// is built off the input path and shared by all sessions using a dictionary.
    pub fn recall(&self, prefix: &str, limit: usize) -> Vec<Candidate> {
        let mut found = BTreeMap::<String, (usize, u32)>::new();
        for (offset, _) in prefix.char_indices().rev().take(8) {
            let suffix = &prefix[offset..];
            let start = self.words.partition_point(|(w, _)| w.as_str() < suffix);
            for (text, weight) in self.words[start..]
                .iter()
                .take(1024)
                .take_while(|(w, _)| w.starts_with(suffix))
            {
                let remaining = &text[suffix.len()..];
                if !remaining.is_empty() {
                    let score = (suffix.chars().count(), *weight);
                    found
                        .entry(remaining.into())
                        .and_modify(|old| *old = (*old).max(score))
                        .or_insert(score);
                }
            }
        }
        for line in ASSOCIATIONS.lines().filter(|l| !l.starts_with('#')) {
            if let Some((before, next)) = line.split_once('\t') {
                if prefix.ends_with(before) && word(next) {
                    found
                        .entry(next.into())
                        .or_insert((before.chars().count(), 1000));
                }
            }
        }
        let mut items: Vec<_> = found.into_iter().collect();
        items.sort_by(|a, b| {
            b.1 .0
                .cmp(&a.1 .0)
                .then(b.1 .1.cmp(&a.1 .1))
                .then(a.0.cmp(&b.0))
        });
        items
            .into_iter()
            .take(limit.min(64))
            .enumerate()
            .map(|(id, (text, _))| Candidate {
                id: CandidateId(id),
                text,
                exact: true,
                hints: Vec::new(),
            })
            .collect()
    }
}
fn word(value: &str) -> bool {
    (1..=16).contains(&value.chars().count())
        && value
            .chars()
            .all(|c| matches!(c, '\u{3400}'..='\u{9fff}' | '\u{20000}'..='\u{323af}'))
}

#[derive(Clone)]
pub struct Prediction {
    pub query: QueryRevision,
    pub items: Vec<Candidate>,
}
impl Prediction {
    pub fn new(items: Vec<Candidate>) -> Self {
        Self {
            query: QueryRevision {
                engine: 0,
                version: NEXT_VIEW.fetch_add(1, Ordering::Relaxed),
            },
            items,
        }
    }
    pub fn window(&self, offset: usize, count: usize) -> CandidateWindow {
        CandidateWindow {
            query_revision: self.query,
            view_revision: self.query.version,
            total: self.items.len(),
            offset: offset.min(self.items.len()),
            candidates: self
                .items
                .iter()
                .skip(offset)
                .take(count.min(MAX_WINDOW))
                .cloned()
                .collect(),
        }
    }
    pub fn view(&self) -> SessionView {
        SessionView {
            visible: !self.items.is_empty(),
            prediction: true,
            items: self
                .items
                .iter()
                .map(|c| PresentationItem {
                    text: c.text.clone(),
                    hint: String::new(),
                })
                .collect(),
            total_pages: usize::from(!self.items.is_empty()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recall_distinguishes_word_remainders_and_next_words_without_repeating_prefix() {
        let lexicon = Lexicon::new(&[
            Entry {
                code: "a".into(),
                word: "北京".into(),
                weight: 10,
            },
            Entry {
                code: "b".into(),
                word: "北京".into(),
                weight: 5,
            },
        ]);
        assert_eq!(
            lexicon
                .recall("北", 5)
                .iter()
                .filter(|c| c.text == "京")
                .count(),
            1
        );
        let next = lexicon.recall("北京", 5);
        assert!(next.iter().any(|c| c.text == "天气"));
        assert!(!next.iter().any(|c| c.text == "北京" || c.text == "京"));
        assert!(lexicon.recall("", 5).is_empty());
    }
}
