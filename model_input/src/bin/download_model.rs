//! Acceptance/recovery CLI using the same downloader as both settings surfaces.
use model_input::download::{self, Source};
use std::sync::atomic::{AtomicBool, Ordering};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: download-model STORE MODEL_ID hf|mirror [CANCEL_AFTER_BYTES]".into());
    }
    let source = match args[2].as_str() {
        "hf" => Source::HuggingFace,
        "mirror" => Source::HfMirror,
        _ => return Err("unknown source".into()),
    };
    let model = download::available_models()
        .into_iter()
        .find(|m| m.manifest.id == args[1])
        .ok_or("unknown model")?;
    let cancel = AtomicBool::new(false);
    let stop = args.get(3).map(|s| s.parse::<u64>()).transpose()?;
    let mut last = (String::new(), 0);
    download::download(
        &model,
        source,
        std::path::Path::new(&args[0]),
        &cancel,
        |p| {
            if p.phase != last.0 || p.completed / (8 * 1024 * 1024) != last.1 {
                if let Ok(line) = serde_json::to_string(&p) {
                    println!("{line}");
                }
                last = (p.phase.clone(), p.completed / (8 * 1024 * 1024));
            }
            if stop.is_some_and(|n| p.completed >= n) {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )?;
    Ok(())
}
