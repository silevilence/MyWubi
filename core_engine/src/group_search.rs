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
pub const WORK_BUDGET: usize = 100_000;
pub const RESULT_BUDGET: usize = 10_000;
const BYTE_BUDGET: usize = 64 * 1024 * 1024;

pub fn query(dict: &Dictionary, positions: &[Position], budget: usize) -> Matches {
    if let Some(index) = dict.pinyin_group_index() {
        return index.search_groups(dict.entries(), positions, budget);
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
) -> Matches {
    let mut result = Matches {
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
        let additional = entry.code.len()
            + if words.contains_key(entry.word.as_str()) {
                0
            } else {
                entry.word.len() + std::mem::size_of::<Match>()
            };
        if bytes + additional > BYTE_BUDGET {
            result.status = SearchStatus::BudgetExceeded;
            break;
        }
        bytes += additional;
        if let Some(&target) = words.get(entry.word.as_str()) {
            let item = &mut result.candidates[target];
            if !item.codes.contains(&entry.code) {
                item.codes.push(entry.code.clone());
            }
        } else {
            if result.candidates.len() >= RESULT_BUDGET {
                result.status = SearchStatus::BudgetExceeded;
                break;
            }
            words.insert(&entry.word, result.candidates.len());
            result.candidates.push(Match {
                text: entry.word.clone(),
                codes: vec![entry.code.clone()],
                exact: !not_exact,
                weight: entry.weight,
            });
        }
    }
    result
}
