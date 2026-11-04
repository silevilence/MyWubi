//! Reproducible ranking and next-word baselines, with labels kept out of inputs.
use std::collections::{BTreeMap, HashSet};

use core_engine::{Dictionary, SearchOptions};
use serde::{Deserialize, Serialize};

/// A committed prefix only; no suffix, target or teacher answer belongs here.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub prefix: String,
    pub code: String,
    pub candidates: Vec<String>,
}

/// Authored replay case. `group` identifies the source document across splits.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub id: String,
    pub group: String,
    pub split: String,
    pub category: String,
    pub task: String,
    pub dictionary: String,
    pub input: Input,
    pub target: String,
}

/// Fail instead of silently evaluating leaked, malformed or duplicate cases.
pub fn validate(samples: &[Sample]) -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut groups = BTreeMap::new();
    let mut texts = BTreeMap::new();
    for s in samples {
        if !ids.insert(&s.id) || s.target.is_empty() || s.group.is_empty() {
            return Err(format!("invalid/duplicate sample: {}", s.id));
        }
        if !["train", "dev", "test"].contains(&s.split.as_str())
            || !["rank", "predict"].contains(&s.task.as_str())
        {
            return Err(format!("invalid split/task: {}", s.id));
        }
        for (map, key) in [
            (&mut groups, s.group.clone()),
            (&mut texts, format!("{}{}", s.input.prefix, s.target)),
        ] {
            if map.insert(key, &s.split).is_some_and(|old| old != &s.split) {
                return Err(format!("cross-split leakage: {}", s.id));
            }
        }
        let unique: HashSet<_> = s.input.candidates.iter().collect();
        if unique.len() != s.input.candidates.len()
            || s.input.candidates.iter().any(String::is_empty)
            || s.input.candidates.len() > 64
            || (s.task == "rank" && s.input.code.is_empty())
            || (s.task == "predict" && !s.input.code.is_empty())
        {
            return Err(format!("invalid input: {}", s.id));
        }
    }
    Ok(())
}

/// Retrieve the real core's legal ordering, including pinyin boundary rules.
pub fn dictionary_baseline(dictionary: &Dictionary, input: &Input) -> Vec<String> {
    dictionary
        .search(
            &input.code,
            SearchOptions {
                limit: 64,
                prefer_exact: true,
            },
        )
        .into_iter()
        .map(|entry| entry.word.clone())
        .collect()
}

/// One- and two-character suffix statistics, learned exclusively from train labels.
#[derive(Default)]
pub struct WordStatistics(BTreeMap<String, BTreeMap<String, usize>>);

impl WordStatistics {
    pub fn train(samples: &[Sample]) -> Self {
        let mut result = Self::default();
        for s in samples
            .iter()
            .filter(|s| s.split == "train" && s.task == "predict")
        {
            for suffix in suffixes(&s.input.prefix) {
                *result
                    .0
                    .entry(suffix)
                    .or_default()
                    .entry(s.target.clone())
                    .or_default() += 1;
            }
        }
        result
    }

    pub fn recall(&self, prefix: &str, limit: usize) -> Vec<String> {
        let mut scores = BTreeMap::new();
        for suffix in suffixes(prefix) {
            if let Some(counts) = self.0.get(&suffix) {
                for (word, count) in counts {
                    *scores.entry(word.clone()).or_insert(0) += count * suffix.chars().count();
                }
            }
        }
        let mut entries: Vec<_> = scores.into_iter().collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        entries
            .into_iter()
            .take(limit)
            .map(|(word, _)| word)
            .collect()
    }
}

fn suffixes(prefix: &str) -> Vec<String> {
    let chars: Vec<_> = prefix.chars().rev().take(2).collect();
    (1..=chars.len())
        .map(|n| chars[..n].iter().rev().collect())
        .collect()
}

/// Raw counts make denominators auditable. Missing recall is separate from rank loss.
#[derive(Debug, Default, Serialize)]
pub struct Metrics {
    pub samples: usize,
    pub recalled: usize,
    pub top1: usize,
    pub top3: usize,
    pub baseline_correct: usize,
    pub correct_to_wrong: usize,
    pub wrong_to_correct: usize,
    pub selection_actions: usize,
    pub page_actions: usize,
}

impl Metrics {
    pub fn observe(
        &mut self,
        target: &str,
        baseline: &[String],
        ranked: &[String],
        page_size: usize,
    ) {
        self.samples += 1;
        let before = baseline.first().is_some_and(|s| s == target);
        let after = ranked.first().is_some_and(|s| s == target);
        self.baseline_correct += usize::from(before);
        self.correct_to_wrong += usize::from(before && !after);
        self.wrong_to_correct += usize::from(!before && after);
        if let Some(index) = ranked.iter().position(|s| s == target) {
            self.recalled += 1;
            self.top1 += usize::from(index == 0);
            self.top3 += usize::from(index < 3);
            self.selection_actions += 1;
            self.page_actions += index / page_size.max(1);
        }
    }
}
