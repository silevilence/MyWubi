//! Small offline comparison model. It sees the same past samples as the suffix
//! model; vocabulary and weights are discarded after each evaluation.
use personal_input::{learning, samples::Sample};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const FEATURES: usize = 256;
const HIDDEN: usize = 16;
const CLASSES: usize = 128;

pub struct Network {
    embeddings: Vec<[f32; HIDDEN]>,
    output: Vec<[f32; HIDDEN]>,
    words: Vec<String>,
}

fn features(left: &str, right: &str) -> Vec<usize> {
    left.chars()
        .rev()
        .take(4)
        .enumerate()
        .chain(right.chars().take(2).enumerate().map(|(i, c)| (4 + i, c)))
        .map(|(position, c)| ((c as usize).wrapping_mul(31) + 43 * position) % FEATURES)
        .collect()
}
impl Network {
    pub fn train(history: &[Sample]) -> Option<Self> {
        let deadline = Instant::now() + Duration::from_secs(learning::BATCH_SECONDS);
        let mut supported: Vec<_> = history
            .iter()
            .rev()
            .filter(|s| core_engine::assistance::prediction::valid_proposal(&s.left, &s.target))
            .take(learning::MAX_SAMPLES)
            .collect();
        supported.reverse();
        let mut counts = BTreeMap::new();
        for s in &supported {
            *counts.entry(s.target.as_str()).or_insert(0u64) += u64::from(s.weight);
        }
        let mut vocab: Vec<_> = counts.into_iter().collect();
        vocab.sort_by(|(a, an), (b, bn)| bn.cmp(an).then(a.cmp(b)));
        let words: Vec<_> = vocab
            .into_iter()
            .take(CLASSES)
            .map(|(w, _)| w.to_owned())
            .collect();
        let indices: BTreeMap<_, _> = words
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, w)| (w, i))
            .collect();
        let mut seed = 7u32;
        let mut row = || {
            std::array::from_fn(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed as f32 / u32::MAX as f32 - 0.5) * 0.2
            })
        };
        let mut net = Self {
            embeddings: (0..FEATURES).map(|_| row()).collect(),
            output: (0..words.len()).map(|_| row()).collect(),
            words,
        };
        for _ in 0..4 {
            for s in &supported {
                if Instant::now() >= deadline {
                    return None;
                }
                let Some(&target) = indices.get(&s.target) else {
                    continue;
                };
                let input = features(&s.left, &s.right);
                let hidden = net.hidden(&input);
                let scores = net.probabilities(&hidden);
                let mut grad = [0.; HIDDEN];
                let rate = 0.05 * s.weight as f32;
                for (i, (weights, p)) in net.output.iter_mut().zip(scores).enumerate() {
                    let delta = p - if i == target { 1. } else { 0. };
                    for ((g, w), h) in grad.iter_mut().zip(weights).zip(hidden) {
                        *g += delta * *w * (1. - h * h);
                        *w -= rate * delta * h;
                    }
                }
                for feature in input {
                    for (w, g) in net.embeddings[feature].iter_mut().zip(grad) {
                        *w -= rate * g;
                    }
                }
            }
        }
        (Instant::now() < deadline).then_some(net)
    }
    fn hidden(&self, input: &[usize]) -> [f32; HIDDEN] {
        std::array::from_fn(|j| {
            input
                .iter()
                .map(|&i| self.embeddings[i][j])
                .sum::<f32>()
                .tanh()
        })
    }
    fn probabilities(&self, hidden: &[f32; HIDDEN]) -> Vec<f32> {
        let logits: Vec<_> = self
            .output
            .iter()
            .map(|row| row.iter().zip(hidden).map(|(w, h)| w * h).sum::<f32>())
            .collect();
        let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut p: Vec<_> = logits.iter().map(|v| (v - max).exp()).collect();
        let sum = p.iter().sum::<f32>();
        for value in &mut p {
            *value /= sum;
        }
        p
    }
    pub fn predict(&self, s: &Sample) -> Vec<String> {
        let scores = self.probabilities(&self.hidden(&features(&s.left, &s.right)));
        let mut ranked: Vec<_> = self
            .words
            .iter()
            .zip(scores)
            .filter(|(w, _)| {
                core_engine::assistance::prediction::valid_proposal(&s.left, w)
                    && personal_input::learning::fits_right(w, &s.right)
            })
            .collect();
        ranked.sort_by(|(a, p), (b, q)| q.total_cmp(p).then(a.cmp(b)));
        ranked
            .into_iter()
            .take(3)
            .map(|(word, _)| word.clone())
            .collect()
    }
    pub fn parameter_bytes(&self) -> usize {
        (self.embeddings.len() + self.output.len()) * HIDDEN * std::mem::size_of::<f32>()
    }
}
