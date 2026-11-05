//! Offline paired evaluation. Only prefix and dictionary recall reach the model.
use core_engine::assistance::*;
use serde::Deserialize;
use std::{
    path::Path,
    time::{Duration, Instant},
};
#[derive(Deserialize)]
struct Data {
    samples: Vec<Sample>,
}
#[derive(Deserialize)]
struct Sample {
    id: String,
    prefix: String,
}
fn control() -> ComputeControl {
    ComputeControl {
        cancellation: Cancellation::default(),
        deadline: Instant::now() + Duration::from_secs(30),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("evaluate-generation MODEL POLICY DATA.json".into());
    }
    let policy: model_input::generation::GenerationPolicy =
        serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let hash = model_input::package::sha256(Path::new(&args[1]))?;
    if !policy.validates(&hash) {
        return Err("model/policy mismatch".into());
    }
    let data: Data = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let backend = model_input::llama::LlamaScorer::load(Path::new(&args[1]), 1024, 2)?
        .with_generation(policy.clone());
    let mut model = ModelSession::new(backend);
    let lexicon = prediction::Lexicon::new(&[]);
    let template: ScoreRequest =
        serde_json::from_str(include_str!("../../evaluation/request.json"))?;
    let mut runs = Vec::new();
    for (i, sample) in data.samples.iter().enumerate() {
        let mut request = template.clone();
        request.key.request = i as u64;
        request.task = Task::Predict;
        request.code.clear();
        request.prefix = sample.prefix.clone();
        request.candidates = lexicon
            .recall(&sample.prefix, 64)
            .into_iter()
            .map(|c| ScoringCandidate {
                id: c.id,
                text: c.text,
            })
            .collect();
        let start = Instant::now();
        let scores = model.score(&request, &control());
        let score_ms = start.elapsed().as_secs_f64() * 1000.;
        let score_rows:Vec<_>=scores.as_ref().map(|r| r.scores.iter().map(|s| serde_json::json!({"text":request.candidates.iter().find(|c| c.id==s.id).map(|c| c.text.as_str()),"score":s.value})).collect()).unwrap_or_default();
        let start = Instant::now();
        let proposal = ProposalRequest {
            key: request.key,
            language: "zh".into(),
            prefix: request.prefix.clone(),
            limit: 3,
        };
        let result = model.propose(&proposal, &control());
        let generate_ms = start.elapsed().as_secs_f64() * 1000.;
        let (words, error) = match result {
            Ok(r) => (r.words, None),
            Err(e) => (vec![], Some(e)),
        };
        let mut chain = Vec::new();
        let mut prefix = sample.prefix.clone();
        if i < 8 {
            if let Some(first) = words.first() {
                prefix.push_str(first);
                chain.push(first.clone());
                for _ in 0..2 {
                    let next = ProposalRequest {
                        prefix: prefix.clone(),
                        ..proposal.clone()
                    };
                    let Ok(response) = model.propose(&next, &control()) else {
                        break;
                    };
                    let Some(word) = response.words.first() else {
                        break;
                    };
                    prefix.push_str(word);
                    chain.push(word.clone());
                }
            }
        }
        runs.push(serde_json::json!({"id":sample.id,"prefix":sample.prefix,"dictionary":request.candidates.iter().map(|c|c.text.as_str()).collect::<Vec<_>>(),"scores":score_rows,"score_error":scores.err(),"score_ms":score_ms,"generated":words,"generation_error":error,"generation_ms":generate_ms,"chain":chain}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"model_sha256":hash,"policy":policy,"dictionary":"built-in prediction lexicon, no user/main table","runs":runs})
        )?
    );
    Ok(())
}
