//! Explicit offline scoring of a request file; never prints request text.
use core_engine::assistance::*;
use model_input::llama::LlamaScorer;
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: score-llama MODEL.gguf REQUEST.json".into());
    }
    let request: ScoreRequest = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let start = Instant::now();
    let scorer = LlamaScorer::load(Path::new(&args[1]), 1024, 2)?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.;
    let mut session = ModelSession::new(scorer);
    let mut runs = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let response = session.score(
            &request,
            &ComputeControl {
                cancellation: Cancellation::default(),
                deadline: start + Duration::from_secs(60),
            },
        )?;
        runs.push(serde_json::json!({"elapsed_ms":start.elapsed().as_secs_f64()*1000.,"response":response}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"load_ms":load_ms,"context_tokens":1024,"threads":2,"gpu_layers":0,"runs":runs})
        )?
    );
    Ok(())
}
