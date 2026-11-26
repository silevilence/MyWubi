//! Explicit fixed-input validation. The IME never performs device discovery or benchmarks.
use crate::{
    onnx::OnnxScorer,
    package::{self, ValidatedPackage},
};
use core_engine::assistance::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
pub struct Request {
    pub package: PathBuf,
    pub profile_dir: PathBuf,
    pub threads: u8,
    pub wait_ms: u16,
    pub max_candidates: u8,
    pub provider: bool,
    pub accelerator_only: bool,
}

pub(crate) fn scorer(
    package: &ValidatedPackage,
    threads: u8,
    profile: Option<&Path>,
) -> Result<OnnxScorer, Unavailable> {
    let manifest = &package.manifest;
    if manifest.backend != package::Backend::OnnxRuntime
        || manifest.capabilities.proposes_words
        || manifest
            .android_execution
            .as_ref()
            .is_none_or(|e| e.provider != "nnapi")
    {
        return Err(Unavailable::Capability);
    }
    let vocabulary = manifest
        .vocabulary
        .as_ref()
        .ok_or(Unavailable::UnsupportedModel)?;
    OnnxScorer::load_session(
        &package.root.join(&manifest.model),
        &package.root.join(vocabulary),
        usize::from(threads),
        true,
        profile,
    )
}

fn placement(counts: &BTreeMap<String, usize>, accelerator_only: bool) -> &'static str {
    let cpu = counts.contains_key("CPUExecutionProvider");
    let nnapi = counts.contains_key("NnapiExecutionProvider");
    match (counts.len(), cpu, nnapi, accelerator_only) {
        (1, true, false, _) => "CPU",
        (1, false, true, true) => "NPU",
        (2, true, true, true) => "CPU + NPU",
        _ => "执行设备未确认",
    }
}

#[derive(Serialize)]
struct Samples {
    load_us: u128,
    words: Vec<Vec<String>>,
    times_us: Vec<u128>,
    errors: Vec<Option<String>>,
}

fn measure(backend: Box<dyn ModelBackend>, load_us: u128, r: &Request) -> Samples {
    let mut session = ModelSession::new(backend);
    let mut samples = Samples {
        load_us,
        words: vec![],
        times_us: vec![],
        errors: vec![],
    };
    for prefix in ["北京", "天气", "明天去", "北京", "天气", "明天去"] {
        let start = Instant::now();
        let response = crate::trial::predict_sample(
            &mut session,
            prefix.into(),
            r.wait_ms.into(),
            r.max_candidates.into(),
        );
        samples.times_us.push(start.elapsed().as_micros());
        match response {
            Ok((words, _)) => {
                samples.words.push(words);
                samples.errors.push(None);
            }
            Err(e) => {
                samples.words.push(vec![]);
                samples.errors.push(Some(e));
            }
        }
    }
    samples
}

fn attributed(package: &ValidatedPackage, r: &Request) -> Result<(Value, Samples), String> {
    let start = Instant::now();
    let mut model = scorer(package, r.threads, Some(&r.profile_dir.join("nodes")))
        .map_err(|e| format!("NNAPI_LOAD: {e}"))?;
    let load_us = start.elapsed().as_micros();
    let mut fixed: ScoreRequest = serde_json::from_str(include_str!("../evaluation/request.json"))
        .map_err(|e| e.to_string())?;
    fixed.code.clear();
    fixed.task = Task::Predict;
    model
        .score(
            &fixed,
            &ComputeControl {
                cancellation: Default::default(),
                deadline: Instant::now() + Duration::from_secs(3),
            },
        )
        .map_err(|e| format!("NNAPI_PREDICT: {e}"))?;
    let path = model
        .finish_profile()
        .map_err(|e| format!("NNAPI_PROFILE: {e}"))?;
    let events: Vec<Value> =
        serde_json::from_reader(fs::File::open(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut counts = BTreeMap::new();
    for event in events.iter().filter(|e| {
        e["cat"] == "Node"
            && e["name"]
                .as_str()
                .is_some_and(|s| s.ends_with("_kernel_time"))
    }) {
        let provider = event["args"]["provider"].as_str().unwrap_or("unknown");
        *counts.entry(provider.to_owned()).or_default() += 1;
    }
    let actual = placement(&counts, r.accelerator_only);
    let gated = package::gate(package, Box::new(model)).map_err(|e| e.to_string())?;
    Ok((
        json!({"actual":actual,"node_events":counts}),
        measure(gated, load_us, r),
    ))
}

fn run(r: &Request) -> Result<Value, String> {
    if !(1..=4).contains(&r.threads)
        || !(50..=1000).contains(&r.wait_ms)
        || !(1..=64).contains(&r.max_candidates)
    {
        return Err("INVALID_BUDGET".into());
    }
    let package = crate::compatibility::check(&r.package).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let cpu =
        package::load_with_threads(&package, r.threads).map_err(|e| format!("CPU_LOAD: {e}"))?;
    let cpu = measure(cpu, start.elapsed().as_micros(), r);
    let reason = if package.manifest.backend == package::Backend::LlamaCpp {
        Some("GGUF_CPU_ONLY")
    } else if package
        .manifest
        .android_execution
        .as_ref()
        .is_none_or(|e| e.provider != "nnapi")
    {
        Some("PACKAGE_CPU_ONLY")
    } else if !r.provider {
        Some("NNAPI_UNAVAILABLE")
    } else {
        None
    };
    let mut report = json!({"cpu":cpu,"task":if package.manifest.capabilities.proposes_words {"generation"} else {"scoring"},
        "backend":package.manifest.backend,"actual":"执行设备未确认","reason":reason});
    if reason.is_none() {
        match attributed(&package, r) {
            Ok((allocation, samples)) => {
                report["actual"] = allocation["actual"].clone();
                report["node_events"] = allocation["node_events"].clone();
                report["nnapi"] = json!(samples);
            }
            Err(error) => report["reason"] = json!(error),
        }
    } else if cpu.errors.iter().any(Option::is_none) {
        report["actual"] = json!("CPU");
    }
    Ok(report)
}

pub fn exchange(bytes: &[u8]) -> Vec<u8> {
    let result = serde_json::from_slice(bytes)
        .map_err(|_| "INVALID_REQUEST".to_owned())
        .and_then(|r| run(&r));
    let reply = match result {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => json!({"ok":false,"error":error}),
    };
    reply.to_string().into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_and_eligible_device_types_are_both_required() {
        let mut nodes = BTreeMap::from([("NnapiExecutionProvider".into(), 4)]);
        assert_eq!(placement(&nodes, false), "执行设备未确认");
        assert_eq!(placement(&nodes, true), "NPU");
        nodes.insert("CPUExecutionProvider".into(), 1);
        assert_eq!(placement(&nodes, true), "CPU + NPU");
        nodes.remove("NnapiExecutionProvider");
        assert_eq!(placement(&nodes, true), "CPU");
        nodes.clear();
        assert_eq!(placement(&nodes, true), "执行设备未确认");
    }
}
