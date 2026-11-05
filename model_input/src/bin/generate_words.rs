//! Offline authored-prefix probe. Input is a JSON string array, never test labels.
use core_engine::assistance::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: generate-words MODEL POLICY PREFIXES.json".into());
    }
    let policy: model_input::generation::GenerationPolicy =
        serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let hash = model_input::package::sha256(Path::new(&args[1]))?;
    if !policy.validates(&hash) {
        return Err("policy/model mismatch".into());
    }
    let prefixes: Vec<String> = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let load = Instant::now();
    let backend = model_input::llama::LlamaScorer::load(Path::new(&args[1]), 1024, 2)?
        .with_generation(policy.clone());
    let load_ms = load.elapsed().as_secs_f64() * 1000.;
    let mut session = ModelSession::new(backend);
    let template: ScoreRequest =
        serde_json::from_str(include_str!("../../evaluation/request.json"))?;
    let mut rows = Vec::new();
    for (id, prefix) in prefixes.into_iter().enumerate() {
        let mut key = template.key;
        key.request = id as u64;
        let request = ProposalRequest {
            key,
            prefix,
            language: "zh".into(),
            limit: policy.max_words,
        };
        let start = Instant::now();
        let result = session.propose(
            &request,
            &ComputeControl {
                cancellation: Cancellation::default(),
                deadline: Instant::now() + Duration::from_secs(30),
            },
        );
        let (words, error) = match result {
            Ok(r) => (r.words, None),
            Err(e) => (vec![], Some(e)),
        };
        rows.push(serde_json::json!({"prefix":request.prefix,"words":words,"error":error,"ms":start.elapsed().as_secs_f64()*1000.}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"model_sha256":hash,"policy":policy,"load_ms":load_ms,"rows":rows})
        )?
    );
    Ok(())
}
