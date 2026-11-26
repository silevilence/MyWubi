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

static PLATFORM: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn activate_platform(platform: String) {
    let _ = PLATFORM.set(platform);
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Decision {
    signature: String,
    pub reason: String,
    pub cpu_budget_us: Option<u64>,
}

pub(crate) struct Lease {
    file: PathBuf,
    decision: Decision,
    pub timeouts: u8,
}

impl Lease {
    pub fn cpu_time(&self) -> Duration {
        Duration::from_micros(self.decision.cpu_budget_us.unwrap_or_default())
    }
    pub fn suspend(&mut self, reason: &str) {
        self.decision.cpu_budget_us = None;
        self.decision.reason = reason.into();
        let _ = write_decision(&self.file, &self.decision);
    }
}

fn decision_file(p: &ValidatedPackage) -> PathBuf {
    p.root.with_file_name(format!(
        ".nnapi-{}--{}.json",
        p.manifest.id, p.manifest.version
    ))
}

fn signature(p: &ValidatedPackage, threads: u8, wait_ms: u16, limit: u8) -> Option<String> {
    use sha2::{Digest, Sha256};
    let input =
        serde_json::to_vec(&(PLATFORM.get()?, &p.manifest, threads, wait_ms, limit)).ok()?;
    Some(format!("{:x}", Sha256::digest(input)))
}

fn write_decision(path: &Path, decision: &Decision) -> std::io::Result<()> {
    use std::io::Write;
    let mut tmp =
        tempfile::NamedTempFile::new_in(path.parent().ok_or(std::io::ErrorKind::InvalidInput)?)?;
    serde_json::to_writer(&mut tmp, decision)?;
    tmp.flush()?;
    tmp.persist(path)?;
    Ok(())
}

pub(crate) fn approved(
    p: &ValidatedPackage,
    c: &core_engine::config::PredictionConfig,
) -> Result<Lease, String> {
    if p.manifest.backend == package::Backend::LlamaCpp {
        return Err("GGUF_CPU_ONLY".into());
    }
    if p.manifest
        .android_execution
        .as_ref()
        .is_none_or(|e| e.provider != "nnapi")
    {
        return Err("PACKAGE_CPU_ONLY".into());
    }
    let path = decision_file(p);
    let decision = fs::File::open(&path)
        .ok()
        .and_then(|f| serde_json::from_reader::<_, Decision>(f).ok())
        .filter(|d| {
            signature(p, c.threads, c.wait_ms, c.max_candidates).as_ref() == Some(&d.signature)
        })
        .ok_or("NO_CURRENT_VALIDATION")?;
    if decision.cpu_budget_us.is_none() {
        return Err(decision.reason);
    }
    Ok(Lease {
        file: path,
        decision,
        timeouts: 0,
    })
}

fn qualifies(cpu: &Samples, accelerated: &Samples, actual: &Value) -> bool {
    if actual != "NPU"
        || cpu.words != accelerated.words
        || cpu.words.iter().any(Vec::is_empty)
        || cpu
            .errors
            .iter()
            .chain(&accelerated.errors)
            .any(Option::is_some)
    {
        return false;
    }
    let mut baseline = cpu.times_us.clone();
    let mut trial = accelerated.times_us.clone();
    baseline.sort_unstable();
    trial.sort_unstable();
    baseline.len() == 6
        && trial.len() == 6
        && (trial[2] + trial[3]) * 100 < (baseline[2] + baseline[3]) * 85
        && trial[5] * 100 < baseline[5] * 85
}

fn run(r: &Request) -> Result<Value, String> {
    if !(1..=4).contains(&r.threads)
        || !(50..=1000).contains(&r.wait_ms)
        || !(1..=64).contains(&r.max_candidates)
    {
        return Err("INVALID_BUDGET".into());
    }
    let package = crate::compatibility::check(&r.package).map_err(|e| e.to_string())?;
    let file = decision_file(&package);
    let mut decision = Decision {
        signature: signature(&package, r.threads, r.wait_ms, r.max_candidates).unwrap_or_default(),
        reason: "NO_CURRENT_VALIDATION".into(),
        cpu_budget_us: None,
    };
    write_decision(&file, &decision).map_err(|e| e.to_string())?;
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
    // Until an accelerator trial supplies its own samples, the displayed candidates
    // come from the measured CPU baseline, including when NNAPI cannot load the graph.
    let actual = if cpu.errors.iter().any(Option::is_none) {
        "CPU"
    } else {
        "执行设备未确认"
    };
    let mut report = json!({"cpu":cpu,"task":if package.manifest.capabilities.proposes_words {"generation"} else {"scoring"},
        "backend":package.manifest.backend,"actual":actual,"reason":reason});
    if reason.is_none() {
        match attributed(&package, r) {
            Ok((allocation, samples)) => {
                report["actual"] = allocation["actual"].clone();
                report["node_events"] = allocation["node_events"].clone();
                if qualifies(&cpu, &samples, &allocation["actual"]) {
                    decision.cpu_budget_us =
                        cpu.times_us.iter().max().map(|t| (cpu.load_us + t) as u64);
                    decision.reason = "VALIDATED".into();
                } else {
                    report["reason"] = json!(if allocation["actual"] == "NPU" {
                        "BENCHMARK_GATE"
                    } else {
                        "PLACEMENT_GATE"
                    });
                }
                report["nnapi"] = json!(samples);
            }
            Err(error) => report["reason"] = json!(error),
        }
    }
    if let Some(reason) = report["reason"].as_str() {
        decision.reason = reason.into();
    }
    report["approved"] = json!(decision.cpu_budget_us.is_some());
    write_decision(&file, &decision).map_err(|e| e.to_string())?;
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
pub(crate) mod tests {
    use super::*;
    #[test]
    fn admission_requires_same_nonempty_outputs_and_both_latency_gains() {
        let samples = |t| Samples {
            load_us: 100,
            times_us: vec![t; 6],
            words: vec![vec!["天气".into()]; 6],
            errors: vec![None; 6],
        };
        let cpu = samples(100);
        let mut trial = samples(70);
        assert!(qualifies(&cpu, &trial, &json!("NPU")));
        assert!(!qualifies(&cpu, &trial, &json!("CPU + NPU")));
        trial.times_us[1] = 88;
        assert!(!qualifies(&cpu, &trial, &json!("NPU")));
        trial.times_us[1] = 70;
        trial.words[2].clear();
        assert!(!qualifies(&cpu, &trial, &json!("NPU")));
    }

    #[cfg(windows)]
    #[test]
    fn fixed_prediction_with_missing_provider_uses_real_cpu_and_keeps_auto_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("counts--v1");
        fs::create_dir(&root).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("evaluation/onnx");
        for name in ["counts.onnx", "vocabulary.json"] {
            fs::copy(fixtures.join(name), root.join(name)).unwrap();
        }
        let policy = json!({"version":1,"adapter":"character-step-v1",
            "model_sha256":package::sha256(&root.join("counts.onnx")).unwrap(),
            "vocabulary_sha256":package::sha256(&root.join("vocabulary.json")).unwrap(),
            "minimum_score":-100,"minimum_margin":0,"evidence":"Fixed plumbing sample"});
        fs::write(root.join("policy.json"), policy.to_string()).unwrap();
        let files: Vec<_> = ["counts.onnx", "vocabulary.json", "policy.json"]
            .iter()
            .map(|p| {
                json!({"path":p,"bytes":fs::metadata(root.join(p)).unwrap().len(),
                "sha256":package::sha256(&root.join(p)).unwrap()})
            })
            .collect();
        let cpu =
            OnnxScorer::load(&root.join("counts.onnx"), &root.join("vocabulary.json"), 2).unwrap();
        let manifest = json!({"protocol":1,"id":"counts","version":"v1","backend":"OnnxRuntime",
            "adapter":"character-step-v1","model":"counts.onnx","vocabulary":"vocabulary.json",
            "prediction_policy":"policy.json","source":"MyWubi","license":"MIT","files":files,
            "capabilities":cpu.capabilities(),"android_execution":{"provider":"nnapi","format":"onnx","quantization":"fp32"}});
        fs::write(root.join("manifest.json"), manifest.to_string()).unwrap();
        activate_platform("test-platform".into());
        let r = Request {
            package: root.clone(),
            profile_dir: dir.path().into(),
            threads: 2,
            max_candidates: 8,
            wait_ms: 1000,
            provider: false,
            accelerator_only: false,
        };
        let report = run(&r).unwrap();
        assert_eq!(report["actual"], "CPU");
        assert_eq!(report["approved"], false);
        assert!(report["cpu"]["words"][0]
            .as_array()
            .is_some_and(|w| !w.is_empty()));
        let package = package::validate(&root).unwrap();
        let config = core_engine::config::PredictionConfig {
            threads: 2,
            wait_ms: 1000,
            max_candidates: 8,
            ..Default::default()
        };
        assert_eq!(
            approved(&package, &config).err().as_deref(),
            Some("NNAPI_UNAVAILABLE")
        );
        assert_eq!(
            approved(
                &package,
                &core_engine::config::PredictionConfig {
                    threads: 1,
                    ..config
                }
            )
            .err()
            .as_deref(),
            Some("NO_CURRENT_VALIDATION")
        );
    }

    pub(crate) fn test_lease(file: PathBuf, budget: u64) -> Lease {
        Lease {
            file,
            decision: Decision {
                signature: "test".into(),
                reason: "VALIDATED".into(),
                cpu_budget_us: Some(budget),
            },
            timeouts: 0,
        }
    }

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
