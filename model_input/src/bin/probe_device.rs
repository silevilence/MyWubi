//! Reproducible native load/warmup/prediction measurements using authored text.
use core_engine::{assistance::*, config::*};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("probe-device PACKAGE cpu|vulkan|cuda|auto DEVICE_NAME ITERATIONS".into());
    }
    let config = PredictionConfig {
        device: match args[2].as_str() {
            "cpu" => ModelDevice::Cpu,
            "vulkan" => ModelDevice::Vulkan,
            "cuda" => ModelDevice::Cuda,
            "auto" => ModelDevice::Auto,
            _ => return Err("invalid device".into()),
        },
        device_name: if args[3] == "-" {
            String::new()
        } else {
            args[3].clone()
        },
        ..Default::default()
    };
    let iterations: usize = args[4].parse()?;
    if !(1..=10000).contains(&iterations) {
        return Err("iterations must be 1..10000".into());
    }
    let package = model_input::package::validate(Path::new(&args[1]))?;
    let manifest = package.manifest.clone();
    let state = Arc::new(Mutex::new(model_input::device::Execution::default()));
    let control = || ComputeControl {
        cancellation: Cancellation::default(),
        deadline: Instant::now() + Duration::from_secs(30),
    };
    let start = Instant::now();
    let mut backend = model_input::device::load(package, &config, Arc::clone(&state))?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    backend.warmup(&control())?;
    println!(
        "{}",
        serde_json::json!({"event":"prepared","manifest":manifest,"execution":*state.lock().map_err(|_| "status lock")?,"load_ms":load_ms,"warmup_ms":start.elapsed().as_secs_f64()*1000.})
    );
    let template: ScoreRequest =
        serde_json::from_str(include_str!("../../evaluation/request.json"))?;
    for index in 0..iterations {
        let mut key = template.key;
        key.request = index as u64;
        let request = ProposalRequest {
            key,
            language: "zh".into(),
            prefix: ["北京", "明天去", "我想喝"][index % 3].into(),
            limit: 3,
        };
        let start = Instant::now();
        let result = backend.propose(&request, &control());
        println!(
            "{}",
            serde_json::json!({"event":"prediction","index":index,"prefix":request.prefix,"ms":start.elapsed().as_secs_f64()*1000.,"result":result,"execution":*state.lock().map_err(|_| "status lock")?})
        );
        backend.reset();
    }
    let start = Instant::now();
    drop(backend);
    println!(
        "{}",
        serde_json::json!({"event":"closed","ms":start.elapsed().as_secs_f64()*1000.})
    );
    Ok(())
}
