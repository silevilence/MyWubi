//! Real-process lifecycle probe using a fixed authored prefix, never host text.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use core_engine::{assistance::*, config::*};
    use std::time::{Duration, Instant};
    fn call(value: serde_json::Value) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let bytes = model_ipc::exchange(&serde_json::to_vec(&value)?, Duration::from_secs(1))?;
        let response: serde_json::Value = serde_json::from_slice(&bytes)?;
        if response["ok"] != true {
            return Err(format!("service error: {}", response["error"]).into());
        }
        Ok(response["value"].clone())
    }
    let args: Vec<_> = std::env::args().collect();
    if !(5..=6).contains(&args.len()) {
        return Err(
            "probe-preload STORE ID VERSION on_demand|preload_keep [cpu|vulkan|cuda]".into(),
        );
    }
    let loading = match args[4].as_str() {
        "on_demand" => ModelLoading::OnDemand,
        "preload_keep" => ModelLoading::PreloadKeep,
        _ => return Err("invalid loading policy".into()),
    };
    let config = PredictionConfig {
        enabled: true,
        loading,
        device: match args.get(5).map(String::as_str).unwrap_or("cpu") {
            "cpu" => ModelDevice::Cpu,
            "vulkan" => ModelDevice::Vulkan,
            "cuda" => ModelDevice::Cuda,
            _ => return Err("invalid device".into()),
        },
        model: Some(ModelSelection {
            id: args[2].clone(),
            version: args[3].clone(),
        }),
        wait_ms: 1000,
        idle_seconds: 10,
        ..Default::default()
    };
    let start = Instant::now();
    call(
        serde_json::json!({"op":"Prepare","root":std::fs::canonicalize(&args[1])?,"config":config}),
    )?;
    if loading == ModelLoading::PreloadKeep {
        while call(serde_json::json!({"op":"Status"}))? != "Ready" {
            if start.elapsed() > Duration::from_secs(30) {
                return Err("preload did not become ready".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    println!(
        "{}",
        serde_json::json!({"event":"prepared","loading":loading,"ms":start.elapsed().as_secs_f64()*1000.,"phase":call(serde_json::json!({"op":"Status"}))?,"execution":call(serde_json::json!({"op":"Execution"}))?})
    );
    let mut request: ScoreRequest =
        serde_json::from_str(include_str!("../../evaluation/request.json"))?;
    request.task = Task::Predict;
    request.code.clear();
    request.prefix = "明天去".into();
    request.candidates.clear();
    for n in 0..3 {
        if n == 1 && loading == ModelLoading::OnDemand {
            // The first request may expire during native loading. A "warm"
            // measurement must wait for Ready, not queue into the same load.
            let ready = Instant::now();
            while call(serde_json::json!({"op":"Status"}))? != "Ready" {
                if ready.elapsed() > Duration::from_secs(30) {
                    return Err("on-demand load did not become ready".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            println!(
                "{}",
                serde_json::json!({"event":"ready_before_warm","wait_ms":ready.elapsed().as_secs_f64()*1000.})
            );
        }
        if n == 2 {
            std::thread::sleep(Duration::from_secs(11));
        }
        request.key.request = n;
        let start = Instant::now();
        call(serde_json::json!({"op":"Submit","request":request}))?;
        let mut result = serde_json::Value::Null;
        while start.elapsed() < Duration::from_millis(1100) {
            result = call(serde_json::json!({"op":"Poll","key":request.key}))?;
            if !result.is_null() {
                break;
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        println!(
            "{}",
            serde_json::json!({"event":(["first","warm","after_idle"][n as usize]),"ms":start.elapsed().as_secs_f64()*1000.,"result":result,"phase":call(serde_json::json!({"op":"Status"}))?})
        );
    }
    call(serde_json::json!({"op":"Pause","paused":true}))?;
    let release = Instant::now();
    while call(serde_json::json!({"op":"Status"}))? != "Paused" {
        if release.elapsed() > Duration::from_secs(30) {
            return Err("native work did not finish releasing".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    println!(
        "{}",
        serde_json::json!({"event":"released","phase":call(serde_json::json!({"op":"Status"}))?})
    );
    call(serde_json::json!({"op":"Pause","paused":false}))?;
    let start = Instant::now();
    if loading == ModelLoading::PreloadKeep {
        while call(serde_json::json!({"op":"Status"}))? != "Ready" {
            if start.elapsed() > Duration::from_secs(30) {
                return Err("resume did not become ready".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    println!(
        "{}",
        serde_json::json!({"event":"resumed","ms":start.elapsed().as_secs_f64()*1000.,"phase":call(serde_json::json!({"op":"Status"}))?})
    );
    call(serde_json::json!({"op":"Close"}))?;
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
