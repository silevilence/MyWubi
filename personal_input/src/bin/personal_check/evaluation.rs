//! Offline paired replay. Raw events, features and fitted models stay in memory.
#[path = "network.rs"]
mod network;
use core_engine::Dictionary;
use personal_input::{
    api, exchange,
    learning::{self, Algorithm, Model},
    samples::{self, Sample},
    Change, EditKind, Error, Event, Evidence, Outcome, Result, Source,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Instant,
};

const LIMIT: usize = 3;

#[derive(Default, Serialize)]
struct Counts {
    samples: usize,
    top1: usize,
    top3: usize,
    covered: usize,
    wrong: usize,
}
impl Counts {
    fn add(&mut self, target: &str, predictions: &[String]) {
        self.samples += 1;
        self.top1 += usize::from(predictions.first().is_some_and(|p| p == target));
        let hit = predictions.iter().any(|p| p == target);
        self.top3 += usize::from(hit);
        self.covered += usize::from(!predictions.is_empty());
        self.wrong += usize::from(!hit && !predictions.is_empty());
    }
}

#[derive(Default, Serialize)]
struct Score {
    all: Counts,
    by_edit: BTreeMap<String, Counts>,
    by_past_context: BTreeMap<&'static str, Counts>,
    outside_dictionary: Counts,
    absent_from_training: Counts,
    latency_us_p95: u128,
}
struct History<'a> {
    targets: BTreeSet<&'a str>,
    last_target: BTreeMap<(&'a str, &'a str), &'a str>,
}
impl<'a> History<'a> {
    fn new(samples: &'a [Sample]) -> Self {
        Self {
            targets: samples.iter().map(|s| s.target.as_str()).collect(),
            last_target: samples
                .iter()
                .map(|s| ((s.left.as_str(), s.right.as_str()), s.target.as_str()))
                .collect(),
        }
    }
}
fn score(
    samples: &[Sample],
    words: &BTreeSet<&str>,
    history: &History<'_>,
    mut predict: impl FnMut(&Sample) -> Vec<String>,
) -> Score {
    let mut result = Score::default();
    let mut times = Vec::new();
    for s in samples {
        let start = Instant::now();
        let candidates = predict(s);
        times.push(start.elapsed().as_micros());
        result.all.add(&s.target, &candidates);
        result
            .by_edit
            .entry(s.edit.clone())
            .or_default()
            .add(&s.target, &candidates);
        if !words.contains(s.target.as_str()) {
            result.outside_dictionary.add(&s.target, &candidates);
        }
        if !history.targets.contains(s.target.as_str()) {
            result.absent_from_training.add(&s.target, &candidates);
        }
        let past = match history
            .last_target
            .get(&(s.left.as_str(), s.right.as_str()))
        {
            None => "unseen_context",
            Some(&target) if target == s.target => "repeated_target",
            Some(_) => "changed_target",
        };
        result
            .by_past_context
            .entry(past)
            .or_default()
            .add(&s.target, &candidates);
    }
    times.sort_unstable();
    result.latency_us_p95 = times
        .get((times.len() * 95).div_ceil(100).saturating_sub(1))
        .copied()
        .unwrap_or(0);
    result
}

fn predictions(model: Option<&Model>, s: &Sample) -> Vec<String> {
    model
        .map(|m| m.predict(&s.left, &s.right, LIMIT))
        .unwrap_or_default()
}
fn dictionary(dict: &Dictionary, s: &Sample) -> Vec<String> {
    dict.prediction_lexicon()
        .recall(&s.left, LIMIT)
        .into_iter()
        .map(|c| c.text)
        .collect()
}
fn fused(model: Option<&Model>, dict: &Dictionary, s: &Sample) -> Vec<String> {
    let mut candidates = predictions(model, s);
    for word in dictionary(dict, s) {
        if candidates.len() == LIMIT {
            break;
        }
        if !candidates.contains(&word) && learning::fits_right(&word, &s.right) {
            candidates.push(word);
        }
    }
    candidates
}

/// Cut immediately before the next confirmed commit, at a fixed fraction of
/// confirmed commits. Edits after a cutoff cannot change its training labels.
fn cuts(events: &[Event]) -> Result<[usize; 3]> {
    let commits: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            matches!(
                e.change,
                Change::Commit {
                    outcome: Outcome::Confirmed,
                    ..
                }
            )
            .then_some(i)
        })
        .collect();
    if commits.len() < 5 {
        return Err(Error::Invalid("至少需要五次确认提交以切分时间窗口"));
    }
    Ok([2, 3, 4].map(|n| commits[commits.len() * n / 5]))
}

pub fn run(args: &[String]) -> Result<()> {
    let [package, table] = args else {
        return Err(Error::Invalid("personal_check evaluate PACKAGE DICTIONARY"));
    };
    let bytes = api::read_package(Path::new(package))?;
    let password = std::env::var("MYWUBI_EVAL_PASSWORD").ok();
    let events = samples::chronological(exchange::decode(&bytes, password.as_deref())?);
    drop(bytes);
    let [first, train, test] = cuts(&events)?;
    let dict = Dictionary::load(table).map_err(|_| Error::Invalid("无法读取评测码表"))?;
    let words: BTreeSet<_> = dict.entries().iter().map(|e| e.word.as_str()).collect();
    let _ = dict.prediction_lexicon();
    let training = samples::build(events[..train].iter().cloned()).kept;
    let validation = samples::build(events[train..test].iter().cloned()).kept;
    let holdout = samples::build(events[test..].iter().cloned());
    if validation.is_empty() || holdout.kept.is_empty() {
        return Err(Error::Invalid("验证或保留窗口没有可核对的样本"));
    }
    let trained = History::new(&training);
    let exact = learning::replay_model(&training, Algorithm::ExactSuffix)?;
    let backoff = learning::replay_model(&training, Algorithm::Backoff)?;
    let a = score(&validation, &words, &trained, |s| {
        predictions(exact.as_ref(), s)
    });
    let b = score(&validation, &words, &trained, |s| {
        predictions(backoff.as_ref(), s)
    });
    let algorithm = if b.all.top3 > a.all.top3 && b.all.wrong <= a.all.wrong {
        Algorithm::Backoff
    } else {
        Algorithm::ExactSuffix
    };
    let mut stages = Vec::new();
    for (name, end) in [("first", first), ("update_1", train), ("update_2", test)] {
        let history = samples::build(events[..end].iter().cloned()).kept;
        let seen = History::new(&history);
        let start = Instant::now();
        // Early versions only know their own history. The external validation
        // window is available for algorithm selection at the last update.
        let supported: Vec<_> = history
            .iter()
            .filter(|s| core_engine::assistance::prediction::valid_proposal(&s.left, &s.target))
            .cloned()
            .collect();
        let stage_algorithm = if end == test {
            algorithm
        } else if supported.len() >= learning::MIN_SAMPLES {
            let recent = &supported[supported.len().saturating_sub(learning::MAX_SAMPLES)..];
            learning::compare(
                recent,
                &std::sync::atomic::AtomicBool::new(false),
                start + std::time::Duration::from_secs(learning::BATCH_SECONDS),
            )?
            .selected
        } else {
            Algorithm::ExactSuffix
        };
        let model = learning::replay_model(&history, stage_algorithm)?;
        let fit_us = start.elapsed().as_micros();
        let size = model
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()?
            .map(|b| b.len());
        let data = &holdout.kept;
        stages.push(json!({
            "stage": name,
            "history_samples": history.len(),
            "trainable": supported.len(),
            "algorithm": stage_algorithm,
            "trained": model.as_ref().map(|m| m.sample_count),
            "fit_us": fit_us,
            "model_bytes": size,
            "personal": score(data, &words, &seen, |s| predictions(model.as_ref(), s)),
            "personal_then_dictionary": score(data, &words, &seen, |s| fused(model.as_ref(), &dict, s)),
        }));
    }
    let corrections: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.change {
            Change::Edit {
                related: Some(id),
                operation: EditKind::Delete | EditKind::Undo | EditKind::Replace,
            } if e.evidence == Evidence::Observed => Some(id.as_str()),
            _ => None,
        })
        .collect();
    let corrected_ids: BTreeSet<_> = corrections.iter().copied().collect();
    let corrected_personal = events
        .iter()
        .filter(|e| {
            corrected_ids.contains(e.id.as_str())
                && matches!(
                    e.change,
                    Change::Commit {
                        source: Source::PersonalPrediction,
                        outcome: Outcome::Confirmed,
                        ..
                    }
                )
        })
        .count();
    let history = samples::build(events[..test].iter().cloned()).kept;
    let seen = History::new(&history);
    let start = Instant::now();
    let network = network::Network::train(&history);
    let network_fit_us = start.elapsed().as_micros();
    let neural = network
        .as_ref()
        .map(|net| score(&holdout.kept, &words, &seen, |s| net.predict(s)));
    let baseline = score(&holdout.kept, &words, &seen, |s| dictionary(&dict, s));
    let report = json!({
        "protocol": 1,
        "candidate_limit": LIMIT,
        "events": events.len(),
        "span_hours": (events[events.len()-1].time_ms - events[0].time_ms) as f64 / 3_600_000.,
        "window_events": [first, train-first, test-train, events.len()-test],
        "linked_corrections": corrections.len(),
        "corrected_personal_commits": corrected_personal,
        "validation": { "exact": a, "backoff": b, "selected": algorithm, "models_ready": exact.is_some() && backoff.is_some() },
        "holdout_excluded": holdout.excluded.len(),
        "small_network": { "score": neural, "fit_us": network_fit_us,
            "within_budget": network.is_some(), "parameter_bytes": network.as_ref().map(|net| net.parameter_bytes()) },
        "dictionary": baseline,
        "stages": stages,
        "observed_net_keys_saved": null,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
