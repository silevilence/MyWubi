//! Isolated-worker IPC + confirmed shared-session publication benchmark.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use core_engine::{config::*, runtime::RuntimeSnapshot, session::*, *};
    use std::{
        os::windows::process::CommandExt,
        process::{Child, Command},
        sync::Arc,
        time::{Duration, Instant},
    };
    type Error = Box<dyn std::error::Error>;
    fn call(value: serde_json::Value) -> Result<serde_json::Value, Error> {
        let bytes = model_ipc::exchange(&serde_json::to_vec(&value)?, Duration::from_secs(2))?;
        let result: serde_json::Value = serde_json::from_slice(&bytes)?;
        if result["ok"] != true {
            return Err(format!("worker: {}", result["error"]).into());
        }
        Ok(result["value"].clone())
    }
    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 8 {
        return Err("probe-pipeline STORE ID VERSION cpu|vulkan|cuda|auto on_demand|preload_keep SECONDS MIN_ITERATIONS".into());
    }
    let seconds: u64 = args[6].parse()?;
    let iterations: usize = args[7].parse()?;
    if seconds > 3600 || !(1..=10000).contains(&iterations) {
        return Err("invalid duration/count".into());
    }
    if model_ipc::exchange(br#"{"op":"Status"}"#, Duration::from_millis(100)).is_ok() {
        return Err("an existing model service owns the pipe; run in an isolated session".into());
    }
    let _child = OwnedChild(
        Command::new(std::env::current_exe()?.with_file_name("model-worker.exe"))
            .creation_flags(0x08000000)
            .spawn()?,
    );
    let config = Config {
        prediction: PredictionConfig {
            enabled: true,
            wait_ms: 1000,
            max_candidates: 3,
            loading: match args[5].as_str() {
                "on_demand" => ModelLoading::OnDemand,
                "preload_keep" => ModelLoading::PreloadKeep,
                _ => return Err("invalid loading".into()),
            },
            device: match args[4].as_str() {
                "cpu" => ModelDevice::Cpu,
                "vulkan" => ModelDevice::Vulkan,
                "cuda" => ModelDevice::Cuda,
                "auto" => ModelDevice::Auto,
                _ => return Err("invalid device".into()),
            },
            model: Some(ModelSelection {
                id: args[2].clone(),
                version: args[3].clone(),
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let root = std::fs::canonicalize(&args[1])?;
    let prepare = Instant::now();
    loop {
        if call(serde_json::json!({"op":"Configure","root":root,"config":config.prediction}))
            .is_ok()
        {
            break;
        }
        if prepare.elapsed() > Duration::from_secs(10) {
            return Err("service startup failed".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let wait_ready = || -> Result<(), Error> {
        let start = Instant::now();
        loop {
            let phase = call(serde_json::json!({"op":"Status"}))?;
            if phase == "Ready" {
                return Ok(());
            }
            if phase.is_object() || start.elapsed() > Duration::from_secs(90) {
                return Err(format!("not ready: {phase}").into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    if config.prediction.loading == ModelLoading::PreloadKeep {
        wait_ready()?;
    }
    println!(
        "{}",
        serde_json::json!({"event":"prepared","ms":prepare.elapsed().as_secs_f64()*1000.,"pid":_child.0.id(),"config":config.prediction,"execution":call(serde_json::json!({"op":"Execution"}))?})
    );
    let runtime = RuntimeSnapshot {
        config,
        dict: Dictionary::from_text("v\t北京\t100\n")?,
        revision: 1,
        temporary: Arc::default(),
        expressions: Arc::default(),
        config_path: "fixture".into(),
        system_table_path: "fixture".into(),
        user_table_path: "fixture".into(),
    };
    // Capture idle residency separately before sustained requests in keep mode.
    if seconds > 0 && runtime.config.prediction.loading == ModelLoading::PreloadKeep {
        std::thread::sleep(Duration::from_secs(10));
    }
    let sustained = Instant::now();
    let mut index = 0;
    while index < iterations || sustained.elapsed() < Duration::from_secs(seconds) {
        let mut session = Session::new(&runtime, BehaviorProfile::LegacyV1, HostContext::default());
        let prefix = ["北京", "明天去", "我想喝"][index % 3];
        let edit = session
            .prepare(Action::InsertLiteral(prefix.into()))
            .map_err(|error| format!("prepare: {error:?}"))?;
        let start = Instant::now();
        session.acknowledge(edit.token, HostOutcome::Applied);
        let request = session
            .prediction_request()
            .ok_or("no prediction request")?;
        let key = request.key;
        call(serde_json::json!({"op":"Submit","request":request}))?;
        let mut result = serde_json::Value::Null;
        while start.elapsed() < Duration::from_millis(1000) {
            result = call(serde_json::json!({"op":"Poll","key":key}))?;
            if !result.is_null() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let timeout = result.is_null();
        let accepted = if timeout {
            session.expire_prediction();
            false
        } else {
            session.complete_model_prediction(key, serde_json::from_value(result.clone())?)
        };
        let visible = accepted && session.has_prediction();
        let words: Vec<_> = session
            .candidate_window(0, 3)
            .candidates
            .into_iter()
            .map(|c| c.text)
            .collect();
        println!(
            "{}",
            serde_json::json!({"event":"prediction","index":index,"elapsed_seconds":sustained.elapsed().as_secs_f64(),"prefix":prefix,"ms":start.elapsed().as_secs_f64()*1000.,"timeout":timeout,"visible":visible,"words":words,"result":result})
        );
        if index == 0 {
            wait_ready()?;
        }
        index += 1;
        std::thread::sleep(Duration::from_millis(100));
    }
    call(serde_json::json!({"op":"Close"}))?;
    let end = Instant::now();
    while call(serde_json::json!({"op":"Execution"}))?["actual"].is_string() {
        if end.elapsed() > Duration::from_secs(30) {
            return Err("release timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    println!(
        "{}",
        serde_json::json!({"event":"closed","seconds":sustained.elapsed().as_secs_f64(),"iterations":index})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
