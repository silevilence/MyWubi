//! 全拼前缀索引。分隔符只增加边界约束，不允许跳过字母。
use crate::dictionary::{Entry, SearchOptions};
use std::{cmp::Reverse, collections::HashSet};

#[derive(Debug)]
struct Key {
    letters: String,
    boundaries: Vec<usize>,
    entry: usize,
}

#[derive(Debug, Default)]
pub(crate) struct PinyinIndex(Vec<Key>);

fn split(code: &str) -> (String, Vec<usize>) {
    let mut letters = String::with_capacity(code.len());
    let mut boundaries = Vec::new();
    for c in code.chars() {
        if c == '\'' {
            boundaries.push(letters.len());
        } else {
            letters.push(c);
        }
    }
    (letters, boundaries)
}

impl PinyinIndex {
    pub fn new(entries: &[Entry]) -> Self {
        let mut keys: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(entry, item)| {
                let (letters, boundaries) = split(&item.code);
                Key {
                    letters,
                    boundaries,
                    entry,
                }
            })
            .collect();
        keys.sort_by(|a, b| a.letters.cmp(&b.letters).then(a.entry.cmp(&b.entry)));
        Self(keys)
    }

    /// The existing normalized index owns syllable boundaries; groups never enumerate strings.
    pub fn search_groups(
        &self,
        entries: &[Entry],
        positions: &[crate::group_input::Position],
        budget: usize,
    ) -> crate::group_search::Matches {
        use crate::group_search::{Matches, SearchStatus};
        let invalid = || Matches {
            status: SearchStatus::InvalidInput,
            visited: 0,
            candidates: Vec::new(),
        };
        if positions.len() > crate::group_input::MAX_POSITIONS {
            return invalid();
        }
        let mut letters = Vec::new();
        let mut boundaries = Vec::new();
        let mut separator = false;
        for position in positions {
            let effective = position.effective();
            if effective == "'" {
                if letters.is_empty() || separator {
                    return invalid();
                }
                boundaries.push(letters.len());
                separator = true;
            } else {
                if !effective.chars().all(|c| c.is_ascii_lowercase()) {
                    return invalid();
                }
                letters.push(position.clone());
                separator = false;
            }
        }
        crate::group_search::query_index(
            entries,
            &letters,
            budget,
            self.0.len(),
            |i| {
                let key = &self.0[i];
                (
                    &key.letters,
                    key.entry,
                    boundaries
                        .iter()
                        .all(|b| key.boundaries.binary_search(b).is_ok()),
                )
            },
            |entry| entry.code.bytes().filter(|&c| c != b'\'').count() == letters.len(),
        )
    }

    pub fn search<'a>(
        &self,
        entries: &'a [Entry],
        input: &str,
        opts: SearchOptions,
    ) -> Vec<&'a Entry> {
        if input.is_empty()
            || input.starts_with('\'')
            || input.contains("''")
            || !input.chars().all(|c| c.is_ascii_lowercase() || c == '\'')
        {
            return Vec::new();
        }
        let (letters, boundaries) = split(input);
        let lower = self.0.partition_point(|key| key.letters < letters);
        let mut matching: Vec<_> = self.0[lower..]
            .iter()
            .take_while(|key| key.letters.starts_with(&letters))
            .filter(|key| {
                boundaries
                    .iter()
                    .all(|boundary| key.boundaries.binary_search(boundary).is_ok())
            })
            .collect();
        matching.sort_by_key(|key| {
            (
                opts.prefer_exact && key.letters != letters,
                Reverse(entries[key.entry].weight),
                key.entry,
            )
        });
        let mut seen = HashSet::new();
        matching
            .into_iter()
            .map(|key| &entries[key.entry])
            .filter(|entry| seen.insert(entry.word.as_str()))
            .take(opts.limit)
            .collect()
    }
}

pub(crate) fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && !code.starts_with('\'')
        && !code.ends_with('\'')
        && !code.contains("''")
        && code.chars().all(|c| c.is_ascii_lowercase() || c == '\'')
}
