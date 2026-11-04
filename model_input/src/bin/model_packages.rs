//! Developer package inspection/import/scoring; model files never supply executable code.
use core_engine::assistance::*;
use model_input::package;
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("validate") if args.len() == 2 => {
            let package = package::validate(Path::new(&args[1]))?;
            println!("{}", serde_json::to_string_pretty(&package.manifest)?);
        }
        Some("import") if args.len() == 3 => {
            let path = package::import(Path::new(&args[1]), Path::new(&args[2]))?;
            println!("{}", path.display());
        }
        Some("list") if args.len() == 2 => {
            let paths = package::list(Path::new(&args[1]))?;
            println!("{}", serde_json::to_string_pretty(&paths)?);
        }
        Some("score") if args.len() == 3 => {
            let package = package::validate(Path::new(&args[1]))?;
            let backend = package::load(&package)?;
            let request: ScoreRequest = serde_json::from_slice(&std::fs::read(&args[2])?)?;
            let control = ComputeControl {
                cancellation: Cancellation::default(),
                deadline: Instant::now() + Duration::from_secs(60),
            };
            let response = ModelSession::new(backend).score(&request, &control)?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        Some("switch") if args.len() >= 4 => {
            let mut store = package::ModelStore::open(Path::new(&args[1]))?;
            let mut request: ScoreRequest = serde_json::from_slice(&std::fs::read(&args[2])?)?;
            for selection in &args[3..] {
                let (id, version) = selection.split_once('@').ok_or("expected id@version")?;
                request.key.model = store.select(id, version)?;
                let response = store.score(
                    &request,
                    &ComputeControl {
                        cancellation: Cancellation::default(),
                        deadline: Instant::now() + Duration::from_secs(60),
                    },
                )?;
                println!("{}", serde_json::to_string(&response)?);
            }
        }
        _ => {
            let usage = "usage: model_packages validate DIR | import DIR STORE | list STORE | score DIR REQUEST.json | switch STORE REQUEST.json id@version...";
            return Err(usage.into());
        }
    }
    Ok(())
}
