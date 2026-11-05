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
    #[serde(default)]
    edited_from: Option<String>,
}
#[derive(Deserialize)]
struct Plan {
    scoring_gate: Gate,
    model_sha256: String,
    policy_sha256: String,
    dataset_sha256: String,
    budget_ms: u64,
}
#[derive(Deserialize)]
struct Gate {
    minimum_score: f32,
    minimum_margin: f32,
}
fn admitted(response: &ScoreResponse, request: &ScoreRequest, gate: &Gate) -> Vec<String> {
    let mut scores = response.scores.clone();
    scores.sort_by(|a, b| b.value.total_cmp(&a.value));
    if scores.first().is_none_or(|s| s.value < gate.minimum_score)
        || (scores.len() > 1 && scores[0].value - scores[1].value < gate.minimum_margin)
    {
        return vec![];
    }
    scores
        .iter()
        .filter(|s| s.value >= gate.minimum_score)
        .take(3)
        .filter_map(|s| {
            request
                .candidates
                .iter()
                .find(|c| c.id == s.id)
                .map(|c| c.text.clone())
        })
        .collect()
}
fn control() -> ComputeControl {
    ComputeControl {
        cancellation: Cancellation::default(),
        deadline: Instant::now() + Duration::from_secs(30),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !(4..=5).contains(&args.len()) {
        return Err("evaluate-generation MODEL POLICY DATA.json [FROZEN_PLAN.json]".into());
    }
    let policy: model_input::generation::GenerationPolicy =
        serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let hash = model_input::package::sha256(Path::new(&args[1]))?;
    if !policy.validates(&hash) {
        return Err("model/policy mismatch".into());
    }
    let data: Data = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let plan: Option<Plan> = args
        .get(4)
        .map(|p| -> Result<Plan, Box<dyn std::error::Error>> {
            Ok(serde_json::from_slice(&std::fs::read(p)?)?)
        })
        .transpose()?;
    if let Some(plan) = &plan {
        if plan.model_sha256 != hash
            || plan.policy_sha256 != model_input::package::sha256(Path::new(&args[2]))?
            || plan.dataset_sha256 != model_input::package::sha256(Path::new(&args[3]))?
            || !(1..=1000).contains(&plan.budget_ms)
        {
            return Err("frozen plan/model/policy mismatch".into());
        }
    }
    let scoring_backend = model_input::llama::LlamaScorer::load(Path::new(&args[1]), 1024, 2)?;
    let mut scorer = ModelSession::new(scoring_backend);
    let mut backend = model_input::llama::LlamaScorer::load(Path::new(&args[1]), 1024, 2)?
        .with_generation(policy.clone());
    let prepare = Instant::now();
    backend.warmup(&control())?;
    let prepare_ms = prepare.elapsed().as_secs_f64() * 1000.;
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
        let scores = scorer.score(&request, &control());
        let score_ms = start.elapsed().as_secs_f64() * 1000.;
        let score_rows:Vec<_>=scores.as_ref().map(|r| r.scores.iter().map(|s| serde_json::json!({"text":request.candidates.iter().find(|c| c.id==s.id).map(|c| c.text.as_str()),"score":s.value})).collect()).unwrap_or_default();
        let proposal = ProposalRequest {
            key: request.key,
            language: "zh".into(),
            prefix: request.prefix.clone(),
            limit: 3,
        };
        let edit_check = if let Some(before) = &sample.edited_from {
            let before_request = ProposalRequest {
                prefix: before.clone(),
                ..proposal.clone()
            };
            let _ = model.propose(&before_request, &control());
            let after = model.propose(&proposal, &control());
            model.reset();
            Some(after.map(|r| r.words).map_err(|e| format!("{e:?}")))
        } else {
            None
        };
        let start = Instant::now();
        let result = model.propose(&proposal, &control());
        let generate_ms = start.elapsed().as_secs_f64() * 1000.;
        let (words, error) = match result {
            Ok(r) => (r.words, None),
            Err(e) => (vec![], Some(e)),
        };
        let edit_isolated = edit_check.as_ref().map(|after| {
            *after
                == if let Some(e) = error {
                    Err(format!("{e:?}"))
                } else {
                    Ok(words.clone())
                }
        });
        if edit_isolated == Some(false) {
            return Err(format!("external edit state leaked for {}", sample.id).into());
        }
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
        let mut paired_chains = Vec::new();
        if let Some(plan) = &plan {
            for method in ["dictionary", "scoring", "generation"] {
                let mut prefix = sample.prefix.clone();
                let mut steps = Vec::new();
                for step in 0..3 {
                    let mut next = request.clone();
                    next.prefix = prefix.clone();
                    next.key.context += step + 1;
                    next.candidates = lexicon
                        .recall(&prefix, 64)
                        .into_iter()
                        .map(|c| ScoringCandidate {
                            id: c.id,
                            text: c.text,
                        })
                        .collect();
                    let started = Instant::now();
                    let bounded = ComputeControl {
                        cancellation: Cancellation::default(),
                        deadline: started + Duration::from_millis(plan.budget_ms),
                    };
                    let (choices, failure) = match method {
                        "dictionary" => (
                            next.candidates
                                .iter()
                                .take(3)
                                .map(|c| c.text.clone())
                                .collect(),
                            None,
                        ),
                        "scoring" => match scorer.score(&next, &bounded) {
                            Ok(r) => (admitted(&r, &next, &plan.scoring_gate), None),
                            Err(e) => (vec![], Some(e)),
                        },
                        _ => match model.propose(
                            &ProposalRequest {
                                key: next.key,
                                prefix: prefix.clone(),
                                ..proposal.clone()
                            },
                            &bounded,
                        ) {
                            Ok(r) => (r.words, None),
                            Err(e) => (vec![], Some(e)),
                        },
                    };
                    let selected = choices.first().cloned();
                    steps.push(serde_json::json!({"prefix":prefix,"choices":choices,"selected":selected,"error":failure,"ms":started.elapsed().as_secs_f64()*1000.}));
                    if let Some(word) = selected {
                        prefix.push_str(&word);
                    } else {
                        break;
                    }
                }
                paired_chains
                    .push(serde_json::json!({"method":method,"steps":steps,"final_prefix":prefix}));
            }
        }
        runs.push(serde_json::json!({"id":sample.id,"prefix":sample.prefix,"dictionary":request.candidates.iter().map(|c|c.text.as_str()).collect::<Vec<_>>(),"scores":score_rows,"score_error":scores.err(),"score_ms":score_ms,"generated":words,"generation_error":error,"generation_ms":generate_ms,"chain":chain,"paired_chains":paired_chains,"edited_from":sample.edited_from,"edit_isolated":edit_isolated}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"model_sha256":hash,"policy":policy,"policy_sha256":model_input::package::sha256(Path::new(&args[2]))?,"dataset_sha256":model_input::package::sha256(Path::new(&args[3]))?,"generation_prepare_ms":prepare_ms,"dictionary":"built-in prediction lexicon, no user/main table","runs":runs})
        )?
    );
    Ok(())
}
