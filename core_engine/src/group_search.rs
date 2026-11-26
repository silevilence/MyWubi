//! Group lookup prunes sorted code ranges; no Cartesian expansion or secondary dictionary copy.
use crate::{
    dictionary::{Dictionary, Entry},
    group_input::Position,
};
use serde::{Deserialize, Serialize};
use std::{cmp::Reverse, collections::HashMap, ops::Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchStatus {
    Complete,
    BudgetExceeded,
    InvalidInput,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub text: String,
    pub codes: Vec<String>,
    pub exact: bool,
    pub weight: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Matches {
    pub status: SearchStatus,
    pub visited: usize,
    pub candidates: Vec<Match>,
}

// Keep row indices into the immutable dictionary. Most words have one code,
// so their spelling and hints require no per-result allocation during typing.
#[derive(Debug)]
pub(crate) struct IndexedMatch {
    pub entry: usize,
    pub exact: bool,
    other_codes: Vec<usize>,
}

impl IndexedMatch {
    pub(crate) fn codes<'a>(&'a self, entries: &'a [Entry]) -> impl Iterator<Item = &'a String> {
        std::iter::once(&self.entry)
            .chain(&self.other_codes)
            .map(|&i| &entries[i].code)
    }
}

#[derive(Debug)]
pub(crate) struct IndexedMatches {
    pub status: SearchStatus,
    pub visited: usize,
    pub candidates: Vec<IndexedMatch>,
}

impl IndexedMatches {
    pub(crate) fn into_owned(self, entries: &[Entry]) -> Matches {
        Matches {
            status: self.status,
            visited: self.visited,
            candidates: self
                .candidates
                .into_iter()
                .map(|item| Match {
                    text: entries[item.entry].word.clone(),
                    codes: item.codes(entries).cloned().collect(),
                    exact: item.exact,
                    weight: entries[item.entry].weight,
                })
                .collect(),
        }
    }
}

pub(crate) struct GroupWindow {
    pub dict: std::sync::Arc<Dictionary>,
    pub matches: std::sync::Arc<IndexedMatches>,
}

#[derive(Debug, Default)]
pub(crate) struct InitialGroups([arc_swap::ArcSwapOption<CachedGroup>; 8]);

#[derive(Debug)]
struct CachedGroup {
    characters: String,
    matches: std::sync::Arc<IndexedMatches>,
}

impl InitialGroups {
    /// All words starting on the same ambiguous key share the expensive first
    /// lookup. Keep only row indices, local to this dictionary generation.
    pub(crate) fn lookup(
        &self,
        dict: &Dictionary,
        positions: &[Position],
    ) -> std::sync::Arc<IndexedMatches> {
        use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
        use std::sync::Arc;
        let [first] = positions else {
            return Arc::new(query_positions(dict, positions, WORK_BUDGET));
        };
        if first.selected.is_some() || first.characters.chars().count() < 2 {
            return Arc::new(query_positions(dict, positions, WORK_BUDGET));
        }
        let hash = BuildHasherDefault::<DefaultHasher>::default().hash_one(&first.characters);
        let slot = &self.0[hash as usize % self.0.len()];
        if let Some(hit) = slot
            .load()
            .as_ref()
            .filter(|hit| hit.characters == first.characters)
        {
            return Arc::clone(&hit.matches);
        }
        let matches = Arc::new(query_positions(dict, positions, WORK_BUDGET));
        let bytes = matches.candidates.capacity() * size_of::<IndexedMatch>()
            + matches
                .candidates
                .iter()
                .map(|row| row.other_codes.capacity() * size_of::<usize>())
                .sum::<usize>();
        if bytes <= 1024 * 1024 {
            slot.store(Some(Arc::new(CachedGroup {
                characters: first.characters.clone(),
                matches: Arc::clone(&matches),
            })));
        }
        matches
    }
}
pub const WORK_BUDGET: usize = 100_000;
pub const RESULT_BUDGET: usize = 10_000;
const BYTE_BUDGET: usize = 64 * 1024 * 1024;

pub fn query(dict: &Dictionary, positions: &[Position], budget: usize) -> Matches {
    query_positions(dict, positions, budget).into_owned(dict.entries())
}

pub(crate) fn query_positions(
    dict: &Dictionary,
    positions: &[Position],
    budget: usize,
) -> IndexedMatches {
    if let Some(index) = dict.pinyin_group_index() {
        return index.group_indices(dict.entries(), positions, budget);
    }
    let entries = dict.entries();
    query_index(
        entries,
        positions,
        budget,
        entries.len(),
        |i| (&entries[i].code, i, true),
        |e| e.code.chars().count() == positions.len(),
    )
}

pub(crate) fn query_index<'a>(
    entries: &[Entry],
    positions: &[Position],
    budget: usize,
    key_count: usize,
    key: impl Fn(usize) -> (&'a str, usize, bool),
    exact: impl Fn(&Entry) -> bool,
) -> IndexedMatches {
    let mut result = IndexedMatches {
        status: SearchStatus::Complete,
        visited: 0,
        candidates: Vec::new(),
    };
    if positions.is_empty()
        || positions.len() > crate::group_input::MAX_POSITIONS
        || positions.iter().any(|p| {
            p.characters.is_empty()
                || p.characters.chars().count() > crate::group_input::MAX_GROUP
                || p.selected.is_some_and(|c| !p.characters.contains(c))
        })
    {
        result.status = SearchStatus::InvalidInput;
        return result;
    }
    let budget = budget.min(WORK_BUDGET);
    let mut ranges = vec![Range {
        start: 0,
        end: key_count,
    }];
    for (depth, position) in positions.iter().enumerate() {
        let mut next = Vec::new();
        let mut characters: Vec<_> = position.effective().chars().collect();
        characters.sort_unstable();
        characters.dedup();
        for range in ranges {
            for character in &characters {
                // Bound all binary-search probes, not just successful branches.
                let cost = 2 * (usize::BITS - (range.len().max(1)).leading_zeros()) as usize + 1;
                if result.visited + cost > budget {
                    result.status = SearchStatus::BudgetExceeded;
                    return result;
                }
                result.visited += cost;
                let partition = |inclusive: bool| {
                    let (mut low, mut high) = (range.start, range.end);
                    while low < high {
                        let middle = low + (high - low) / 2;
                        let c = key(middle).0.chars().nth(depth);
                        if c < Some(*character) || (inclusive && c == Some(*character)) {
                            low = middle + 1;
                        } else {
                            high = middle;
                        }
                    }
                    low - range.start
                };
                let low = partition(false);
                let high = partition(true);
                if low < high {
                    next.push(range.start + low..range.start + high);
                }
            }
        }
        ranges = next;
        if ranges.is_empty() {
            return result;
        }
    }

    let mut indices = Vec::new();
    'ranges: for range in ranges {
        for index in range {
            if result.visited >= budget {
                result.status = SearchStatus::BudgetExceeded;
                break 'ranges;
            }
            result.visited += 1;
            let (_, entry, valid) = key(index);
            if valid {
                // Cache the ordering key once. Pinyin exactness counts normalized
                // letters and must not be recalculated by every sort comparison.
                indices.push((
                    !exact(&entries[entry]),
                    Reverse(entries[entry].weight),
                    entry,
                ));
            }
        }
    }
    indices.sort_unstable();
    let mut words = HashMap::<&str, usize>::new();
    let mut bytes = 0usize;
    for (not_exact, _, index) in indices {
        let entry = &entries[index];
        let slot = words.entry(entry.word.as_str());
        let additional = entry.code.len()
            + match slot {
                std::collections::hash_map::Entry::Occupied(_) => 0,
                std::collections::hash_map::Entry::Vacant(_) => {
                    entry.word.len() + std::mem::size_of::<Match>()
                }
            };
        if bytes + additional > BYTE_BUDGET {
            result.status = SearchStatus::BudgetExceeded;
            break;
        }
        bytes += additional;
        match slot {
            std::collections::hash_map::Entry::Occupied(slot) => {
                let item = &mut result.candidates[*slot.get()];
                if !item.codes(entries).any(|code| code == &entry.code) {
                    item.other_codes.push(index);
                }
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                if result.candidates.len() >= RESULT_BUDGET {
                    result.status = SearchStatus::BudgetExceeded;
                    break;
                }
                slot.insert(result.candidates.len());
                result.candidates.push(IndexedMatch {
                    entry: index,
                    other_codes: Vec::new(),
                    exact: !not_exact,
                });
            }
        }
    }
    result
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn alternating_first_groups_keep_their_own_rows_and_explicit_selections() {
        let text = ('a'..='z')
            .map(|c| format!("{c}\t{c}\t1\n"))
            .collect::<String>();
        let dict = Dictionary::from_text(&text).unwrap();
        // Nine distinct groups exercise eviction in an eight-slot dictionary cache.
        let groups = ["ab", "cd", "ef", "gh", "ij", "kl", "mn", "op", "qr"];
        for chars in groups.into_iter().chain(groups.into_iter().rev()) {
            let mut position = Position {
                characters: chars.to_owned(),
                selected: None,
            };
            let first = dict
                .initial_groups
                .lookup(&dict, std::slice::from_ref(&position));
            let again = dict
                .initial_groups
                .lookup(&dict, std::slice::from_ref(&position));
            assert!(Arc::ptr_eq(&first, &again));
            let words: Vec<_> = first
                .candidates
                .iter()
                .map(|item| dict.entries()[item.entry].word.as_str())
                .collect();
            assert_eq!(words.concat(), chars);
            position.selected = chars.chars().next();
            let narrowed = dict.initial_groups.lookup(&dict, &[position]);
            assert_eq!(narrowed.candidates.len(), 1);
            assert_eq!(dict.entries()[narrowed.candidates[0].entry].word, words[0]);
        }
    }

    #[test]
    fn real_layout_first_keys_keep_index_storage_within_budget() {
        for table in ["wubi86", "pinyin"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../tables/{table}.dict"));
            let dict = Dictionary::load(path).unwrap();
            for characters in
                "qw er ty ui op as df gh jk l zx cv bn m abc def ghi jkl mno pqr stu vwx yz"
                    .split_whitespace()
            {
                let position = Position {
                    characters: characters.into(),
                    selected: None,
                };
                dict.initial_groups.lookup(&dict, &[position]);
            }
            let cached: Vec<_> = dict
                .initial_groups
                .0
                .iter()
                .filter_map(|s| s.load_full())
                .collect();
            let mut rows = 0;
            let mut bytes = 0;
            for group in &cached {
                let items = &group.matches.candidates;
                rows += items.len();
                bytes += items.capacity() * size_of::<IndexedMatch>();
                bytes += items
                    .iter()
                    .map(|m| m.other_codes.capacity() * size_of::<usize>())
                    .sum::<usize>();
            }
            assert!(!cached.is_empty());
            assert!(bytes <= 8 * 1024 * 1024);
            eprintln!(
                "{table}: {} cached keys, {rows} rows, {bytes} allocated index bytes",
                cached.len()
            );
        }
    }
}
