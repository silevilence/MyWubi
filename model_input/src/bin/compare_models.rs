//! Compare offline baselines with real adapters. Labels never enter scoring requests.
use core_engine::{
    assistance::*,
    candidates::{CandidateId, QueryRevision},
    session::EditToken,
};
use model_input::{evaluation::*, llama::LlamaScorer, onnx::OnnxScorer};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

fn evaluate<B: ModelBackend>(
    backend: B,
    samples: &[Sample],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut backend = ModelSession::new(backend);
    let stats = WordStatistics::train(samples);
    let mut metrics = BTreeMap::<String, Metrics>::new();
    let mut deadline_metrics = BTreeMap::<String, Metrics>::new();
    let mut runs = Vec::new();
    for (index, s) in samples
        .iter()
        .enumerate()
        .filter(|(_, s)| s.split != "train")
    {
        let candidates = if s.task == "rank" {
            s.input.candidates.clone()
        } else {
            stats.recall(&s.input.prefix, 64)
        };
        let request = ScoreRequest {
            protocol: 1,
            key: RequestKey {
                edit: EditToken {
                    session: 1,
                    generation: index as u64,
                    editor: 1,
                    epoch: 1,
                    runtime: 1,
                },
                query: QueryRevision {
                    engine: 1,
                    version: index as u64,
                },
                view: index as u64,
                context: index as u64,
                model: 1,
                config: 1,
                request: index as u64,
            },
            task: if s.task == "rank" {
                Task::Rank
            } else {
                Task::Predict
            },
            language: "zh".into(),
            prefix: s.input.prefix.clone(),
            code: s.input.code.clone(),
            candidates: candidates
                .iter()
                .enumerate()
                .map(|(i, text)| ScoringCandidate {
                    id: CandidateId(i),
                    text: text.clone(),
                })
                .collect(),
        };
        let mut ranked = candidates.clone();
        for pass in 0..2 {
            let start = Instant::now();
            // Offline quality computation deliberately permits late results, but records
            // whether they would have been usable under the frozen product deadline.
            let result = backend.score(
                &request,
                &ComputeControl {
                    cancellation: Cancellation::default(),
                    deadline: start + Duration::from_secs(60),
                },
            );
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
            let cutoff = if s.task == "rank" { 100. } else { 250. };
            if let Ok(response) = &result {
                let mut scores = response.scores.clone();
                scores.sort_by(|a, b| b.value.total_cmp(&a.value).then(a.id.0.cmp(&b.id.0)));
                ranked = scores.iter().map(|v| candidates[v.id.0].clone()).collect();
            }
            let within_budget = result.is_ok() && elapsed_ms <= cutoff;
            // Prediction deadlines yield no suggestion; ranking deadlines restore
            // the existing base candidates. Do not count a late suggestion as visible.
            let fallback: &[String] = if s.task == "rank" { &candidates } else { &[] };
            deadline_metrics
                .entry(format!("{}/{}/pass{pass}", s.split, s.task))
                .or_default()
                .observe(
                    &s.target,
                    &candidates,
                    if within_budget { &ranked } else { fallback },
                    5,
                );
            runs.push(serde_json::json!({"sample":s.id,"pass":pass,"elapsed_ms":elapsed_ms,"within_budget":within_budget,"scores":result.as_ref().ok().map(|r| &r.scores),"error":result.as_ref().err()}));
        }
        for category in ["all", s.category.as_str()] {
            metrics
                .entry(format!("{}/{}/{}", s.split, s.task, category))
                .or_default()
                .observe(&s.target, &candidates, &ranked, 5);
        }
    }
    Ok(
        serde_json::json!({"metrics_offline_including_late_results":metrics,"metrics_deadline_fallback":deadline_metrics,"runs":runs}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: compare-models MODEL.gguf".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let samples: Vec<Sample> = serde_json::from_str(&std::fs::read_to_string(
        root.join("evaluation/samples.json"),
    )?)?;
    validate(&samples)?;
    let start = Instant::now();
    let llama = LlamaScorer::load(Path::new(&args[1]), 1024, 2)?;
    let llama_load_ms = start.elapsed().as_secs_f64() * 1000.;
    let llama = evaluate(llama, &samples)?;
    let start = Instant::now();
    let onnx = OnnxScorer::load(
        &root.join("evaluation/onnx/counts.onnx"),
        &root.join("evaluation/onnx/vocabulary.json"),
        2,
    )?;
    let onnx_load_ms = start.elapsed().as_secs_f64() * 1000.;
    let onnx = evaluate(onnx, &samples)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"llama_load_ms":llama_load_ms,"onnx_load_ms":onnx_load_ms,"llama":llama,"onnx_authored_count_baseline":onnx,"quality_scope":"38 authored engineering fixtures, not independent product validation"})
        )?
    );
    Ok(())
}
