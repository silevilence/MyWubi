//! Bounded suffix/phrase baseline, experimental until real temporal evaluation.
use crate::{
    samples::{self, Sample},
    store::{digest, Store},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub const MAX_SAMPLES: usize = 20_000;
pub const MAX_MODEL_BYTES: usize = 8 * 1024 * 1024;
pub const MIN_SAMPLES: usize = 20;
pub const BATCH_SECONDS: u64 = 2;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Algorithm {
    ExactSuffix,
    Backoff,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phrase {
    pub left: String,
    pub right: String,
    pub text: String,
    pub count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub trained_events: std::collections::BTreeSet<String>,
    pub format: u32,
    pub version: String,
    pub created_ms: u64,
    pub data_epoch: u64,
    pub data_hash: String,
    pub sample_count: usize,
    pub algorithm: Algorithm,
    pub tokenizer: String,
    pub entries: Vec<Phrase>,
    pub evaluation: Metrics,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub samples: usize,
    pub top1: usize,
    pub top3: usize,
    pub covered: usize,
    pub wrong: usize,
    pub saved_keys: usize,
    pub new_phrases: usize,
    pub latency_us_p50: u64,
    pub latency_us_p95: u64,
    pub latency_us_p99: u64,
}
fn suffix(text: &str, length: usize) -> String {
    text.chars()
        .rev()
        .take(length)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
fn prefix(text: &str, length: usize) -> String {
    text.chars().take(length).collect()
}
impl Model {
    pub fn predict(&self, left: &str, right: &str, limit: usize) -> Vec<String> {
        if left.is_empty() {
            return Vec::new();
        }
        let right = prefix(right, 8);
        let mut scored: BTreeMap<&str, u64> = BTreeMap::new();
        for phrase in &self.entries {
            if !left.ends_with(&phrase.left)
                || phrase.left.is_empty()
                || phrase.right != right
                || !core_engine::assistance::prediction::valid_proposal(left, &phrase.text)
                || (!right.is_empty()
                    && (phrase.text.starts_with(&right) || right.starts_with(&phrase.text)))
            {
                continue;
            }
            let n = phrase.left.chars().count() as u64;
            let score = u64::from(phrase.count) * n * n;
            let value = scored.entry(&phrase.text).or_default();
            *value = (*value).max(score);
        }
        let mut scored: Vec<_> = scored.into_iter().collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        scored
            .into_iter()
            .take(limit.min(16))
            .map(|(s, _)| s.to_owned())
            .collect()
    }
    pub fn valid(&self) -> bool {
        self.format == 1
            && self.tokenizer == "unicode-scalar-v1"
            && self.sample_count >= MIN_SAMPLES
            && self.entries.len() <= MAX_SAMPLES * 4
            && self.entries.iter().all(|p| {
                p.count > 0
                    && !p.text.is_empty()
                    && p.text.chars().count() <= 64
                    && p.left.chars().count() <= 8
                    && p.right.chars().count() <= 8
            })
    }
}
fn fit(
    samples: &[Sample],
    algorithm: Algorithm,
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<Model> {
    let mut counts = BTreeMap::<(String, String, String), u32>::new();
    for (index, sample) in samples.iter().enumerate() {
        if index % 32 == 0 && (cancel.load(Ordering::Acquire) || Instant::now() >= deadline) {
            return Err(Error::Invalid("学习已暂停或达到批次预算"));
        }
        let lengths: &[usize] = match algorithm {
            Algorithm::ExactSuffix => &[8],
            Algorithm::Backoff => &[1, 2, 4, 8],
        };
        let mut keys = std::collections::BTreeSet::new();
        for length in lengths {
            let key = (
                suffix(&sample.left, *length),
                prefix(&sample.right, 8),
                sample.target.clone(),
            );
            if !key.0.is_empty() && keys.insert(key.clone()) {
                *counts.entry(key).or_default() += sample.weight;
            }
        }
    }
    Ok(Model {
        trained_events: samples.iter().map(|s| s.event.clone()).collect(),
        format: 1,
        version: crate::identity()?,
        created_ms: crate::now_ms(),
        data_epoch: 0,
        data_hash: String::new(),
        sample_count: samples.len(),
        algorithm,
        tokenizer: "unicode-scalar-v1".into(),
        entries: counts
            .into_iter()
            .map(|((left, right, text), count)| Phrase {
                left,
                right,
                text,
                count,
            })
            .collect(),
        evaluation: Metrics::default(),
    })
}
pub fn evaluate(model: &Model, samples: &[Sample]) -> Metrics {
    evaluate_checked(
        model,
        samples,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(300),
    )
    .unwrap_or_default()
}
fn evaluate_checked(
    model: &Model,
    samples: &[Sample],
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<Metrics> {
    let mut result = Metrics {
        samples: samples.len(),
        ..Default::default()
    };
    let mut latencies = Vec::new();
    for sample in samples {
        if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(Error::Invalid("评测已暂停或达到批次预算"));
        }
        let start = Instant::now();
        let candidates = model.predict(&sample.left, &sample.right, 3);
        latencies.push(start.elapsed().as_micros() as u64);
        result.covered += usize::from(!candidates.is_empty());
        result.top1 += usize::from(candidates.first() == Some(&sample.target));
        if candidates.contains(&sample.target) {
            result.top3 += 1;
            result.saved_keys += sample.target.chars().count().saturating_sub(1);
            result.new_phrases += usize::from(sample.target.chars().count() > 1);
        } else if !candidates.is_empty() {
            result.wrong += 1;
        }
    }
    latencies.sort_unstable();
    let p = |n: usize| {
        latencies
            .get(latencies.len().saturating_sub(1) * n / 100)
            .copied()
            .unwrap_or(0)
    };
    result.latency_us_p50 = p(50);
    result.latency_us_p95 = p(95);
    result.latency_us_p99 = p(99);
    Ok(result)
}
#[derive(Serialize, Deserialize)]
pub struct Comparison {
    pub exact: Metrics,
    pub backoff: Metrics,
    pub selected: Algorithm,
    pub training: usize,
    pub validation: usize,
    pub quality_accepted: bool,
}
pub fn compare(samples: &[Sample], cancel: &AtomicBool, deadline: Instant) -> Result<Comparison> {
    let split = samples.len() * 4 / 5;
    let (train, validation) = samples.split_at(split);
    let exact = evaluate_checked(
        &fit(train, Algorithm::ExactSuffix, cancel, deadline)?,
        validation,
        cancel,
        deadline,
    )?;
    let backoff = evaluate_checked(
        &fit(train, Algorithm::Backoff, cancel, deadline)?,
        validation,
        cancel,
        deadline,
    )?;
    let selected = if backoff.top3 > exact.top3 && backoff.wrong <= exact.wrong {
        Algorithm::Backoff
    } else {
        Algorithm::ExactSuffix
    };
    Ok(Comparison {
        exact,
        backoff,
        selected,
        training: train.len(),
        validation: validation.len(),
        quality_accepted: false,
    })
}
pub fn load(store: &Store) -> Result<Model> {
    let path = store.root.join("model.json");
    if path.metadata()?.len() > MAX_MODEL_BYTES as u64 {
        return Err(Error::Invalid("个人模型超过资源上限"));
    }
    let model: Model = serde_json::from_slice(&std::fs::read(path)?)?;
    let state = store.snapshot()?;
    let withdrawn=state.events.values().any(|e|matches!(&e.change,crate::Change::Edit{related:Some(id),operation:crate::EditKind::Delete|crate::EditKind::Replace|crate::EditKind::Undo} if e.evidence==crate::Evidence::Observed && model.trained_events.contains(id)));
    if !model.valid() || model.data_epoch != state.data_epoch || withdrawn {
        return Err(Error::Invalid("个人模型失效，需要重新训练"));
    }
    Ok(model)
}
pub fn train(store: &Store, cancel: &AtomicBool) -> Result<Model> {
    let _training = {
        use fs2::FileExt;
        std::fs::create_dir_all(&store.root)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(store.root.join("training.lock"))?;
        file.try_lock_exclusive()
            .map_err(|_| Error::Invalid("已有个人学习任务"))?;
        file
    };
    let deadline = Instant::now() + Duration::from_secs(BATCH_SECONDS);
    let state = store.snapshot()?;
    let hash = digest(&serde_json::to_vec(&state.events)?);
    let old = load(store).ok();
    if let Some(model) = &old {
        if model.data_hash == hash {
            return Ok(model.clone());
        }
    }
    let mut samples = samples::build(state.events.into_values());
    if samples.kept.len() < MIN_SAMPLES {
        return Err(Error::Invalid(
            "待积累：至少需要 20 条已确认且上下文完整的输入",
        ));
    }
    if samples.kept.len() > MAX_SAMPLES {
        samples.kept.drain(..samples.kept.len() - MAX_SAMPLES);
    }
    let comparison = compare(&samples.kept, cancel, deadline)?;
    let split = samples.kept.len() * 4 / 5;
    let candidate = fit(
        &samples.kept[..split],
        comparison.selected,
        cancel,
        deadline,
    )?;
    let metrics = evaluate_checked(&candidate, &samples.kept[split..], cancel, deadline)?;
    if let Some(old) = &old {
        let fresh: Vec<_> = samples.kept[split..]
            .iter()
            .filter(|s| !old.trained_events.contains(&s.event))
            .cloned()
            .collect();
        if fresh.len() < 4 {
            return Err(Error::Invalid("待积累：需要至少 4 条未训练的新验证样本"));
        }
        let current = evaluate_checked(&candidate, &fresh, cancel, deadline)?;
        let previous = evaluate_checked(old, &fresh, cancel, deadline)?;
        if current.top3 < previous.top3 || current.wrong > previous.wrong {
            return Err(Error::Invalid("新模型未通过防退化检查，保留原模型"));
        }
    }
    let mut model = fit(&samples.kept, comparison.selected, cancel, deadline)?;
    model.data_hash = hash;
    model.data_epoch = state.data_epoch;
    model.evaluation = metrics;
    let bytes = serde_json::to_vec(&model)?;
    if bytes.len() > MAX_MODEL_BYTES || !model.valid() {
        return Err(Error::Invalid("个人模型未通过结构或资源检查"));
    }
    let _lock = store.lock()?;
    let current = store.load_locked()?;
    if current.data_epoch != model.data_epoch
        || current.generation != state.generation
        || cancel.load(Ordering::Acquire)
    {
        return Err(Error::Invalid("训练期间数据已替换或任务已暂停"));
    }
    if let Some(old) = old {
        core_engine::data::atomic_write(
            &store.root.join("previous-model.json"),
            &serde_json::to_vec(&old)?,
        )?;
    }
    core_engine::data::atomic_write(
        &store.root.join("training.json"),
        &serde_json::to_vec(&samples)?,
    )?;
    core_engine::data::atomic_write(&store.root.join("model.json"), &bytes)?;
    Ok(model)
}
pub fn rollback(store: &Store) -> Result<()> {
    let _lock = store.lock()?;
    let bytes = std::fs::read(store.root.join("previous-model.json"))?;
    let model: Model = serde_json::from_slice(&bytes)?;
    if !model.valid() || model.data_epoch != store.load_locked()?.data_epoch {
        return Err(Error::Invalid("上一版本不适用于当前数据"));
    }
    core_engine::data::atomic_write(&store.root.join("model.json"), &bytes)?;
    Ok(())
}
