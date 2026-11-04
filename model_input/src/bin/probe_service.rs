//! Developer-only real-process smoke test. Uses authored text, not host input.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use core_engine::{
        assistance::*,
        candidates::{CandidateId, QueryRevision},
        config::{ModelSelection, PredictionConfig},
        session::EditToken,
    };
    use std::time::{Duration, Instant};
    fn call(value: serde_json::Value) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let response = model_ipc::exchange(&serde_json::to_vec(&value)?, Duration::from_secs(1))?;
        let value: serde_json::Value = serde_json::from_slice(&response)?;
        if value["ok"] != true {
            return Err(format!("service error: {}", value["error"]).into());
        }
        Ok(value["value"].clone())
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err("usage: probe-service STORE ID VERSION [SAMPLES 1..100]".into());
    }
    let samples = args
        .get(3)
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(1);
    if !(1..=100).contains(&samples) {
        return Err("samples outside 1..100".into());
    }
    let config = PredictionConfig {
        enabled: true,
        model: Some(ModelSelection {
            id: args[1].clone(),
            version: args[2].clone(),
        }),
        wait_ms: 1000,
        ..Default::default()
    };
    call(
        serde_json::json!({"op":"Configure","root":std::fs::canonicalize(&args[0])?,"config":config}),
    )?;
    call(serde_json::json!({"op":"Pause","paused":false}))?;
    let started = Instant::now();
    let mut cold = None;
    let mut timings = Vec::new();
    let mut attempted = 0;
    for n in 1..=(samples as u64 + 20) {
        attempted += 1;
        let key = RequestKey {
            edit: EditToken {
                session: 1,
                generation: n,
                editor: 1,
                epoch: 1,
                runtime: 1,
            },
            query: QueryRevision {
                engine: 0,
                version: n,
            },
            view: n,
            context: 1,
            model: 1,
            config: 1,
            request: n,
        };
        let request = ScoreRequest {
            protocol: 1,
            key,
            task: Task::Predict,
            language: "zh".into(),
            prefix: "北京".into(),
            code: String::new(),
            candidates: vec![
                ScoringCandidate {
                    id: CandidateId(7),
                    text: "天气".into(),
                },
                ScoringCandidate {
                    id: CandidateId(9),
                    text: "大学".into(),
                },
            ],
        };
        let round = Instant::now();
        call(serde_json::json!({"op":"Submit","request":request}))?;
        while round.elapsed() < Duration::from_millis(1100) {
            if let Some(value) = call(serde_json::json!({"op":"Poll","key":key}))?.as_object() {
                let response: Result<ScoreResponse, Unavailable> =
                    serde_json::from_value(serde_json::Value::Object(value.clone()))?;
                let response = response?;
                response.validate(&request, key)?;
                if response
                    .prediction_accepted
                    .as_ref()
                    .is_none_or(Vec::is_empty)
                {
                    return Err("service returned scores without prediction admission".into());
                }
                timings.push(round.elapsed().as_micros() as f64 / 1000.0);
                if cold.is_none() {
                    println!(
                        "{}",
                        serde_json::json!({"model":args[1],"ready_ms":started.elapsed().as_millis(),"round_ms":round.elapsed().as_millis(),"scores":response.scores})
                    );
                }
                cold = Some(());
                break;
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        if timings.len() == samples {
            break;
        }
    }
    call(serde_json::json!({"op":"Pause","paused":true}))?;
    if timings.len() != samples {
        return Err("service did not complete the requested bounded samples".into());
    }
    timings.sort_by(f64::total_cmp);
    let percentile =
        |p: f64| timings[((timings.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    println!(
        "{}",
        serde_json::json!({"model":args[1],"samples":samples,"attempted":attempted,"valid_ratio":samples as f64/attempted as f64,"p50_ms":percentile(0.50),"p95_ms":percentile(0.95),"p99_ms":percentile(0.99),"scope":"service round trip; authored fixed two-candidate fixture; not keyboard latency or quality"})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
