//! Headless native shaping/raster baseline; deliberately excludes TSF and display latency.
use std::time::Instant;
use text_render::{Draw, TextSystem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/validation/skins/states.json"))?;
    let mut system = TextSystem::new()?;
    let mut results = Vec::new();
    for dpi in [96, 120, 144, 192] {
        let size = 14.0 * dpi as f32 / 72.0;
        for state in fixtures["states"].as_array().ok_or("missing states")? {
            let text = state["text"].as_str().ok_or("missing text")?;
            let mut samples = Vec::new();
            for iteration in 0..220 {
                let start = Instant::now();
                let layout = system.layout(text, "Microsoft YaHei UI", size, 500.0)?;
                let bitmap = system.render(
                    520,
                    layout.height.ceil() as u32 + 16,
                    0xffffffff,
                    &[Draw::Text {
                        layout: &layout,
                        x: 8.0,
                        y: 8.0,
                        color: 0xff202020,
                    }],
                )?;
                std::hint::black_box(&bitmap);
                if iteration >= 20 {
                    samples.push(start.elapsed().as_nanos() as u64);
                }
            }
            samples.sort_unstable();
            results.push(serde_json::json!({"state":state["id"], "dpi":dpi,
                "samples":samples.len(), "p50_ns":samples[99], "p95_ns":samples[189]}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "scope":"Release native shaping + software raster; no HWND, TSF, compositor, or device latency",
            "warmup":20, "results":results
        }))?
    );
    Ok(())
}
