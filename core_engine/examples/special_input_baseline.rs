//! Reproducible SI baseline using local resources; no input-method installation or network.
use core_engine::{
    dictionary::{Dictionary, Entry, LoadOptions},
    expressions::{ExpressionKind, ExpressionLibrary},
    InputEvent, StateMachine,
};
use serde_json::{json, Value};
use std::{hint::black_box, path::Path, sync::Arc, time::Instant};

fn distribution(mut times: Vec<f64>) -> Value {
    times.sort_by(f64::total_cmp);
    let percentile = |p: usize| times[(times.len() * p).div_ceil(100) - 1];
    json!({"samples": times.len(), "p50_us": percentile(50),
        "p95_us": percentile(95), "p99_us": percentile(99)})
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut searches = Vec::new();
    for (kind, file, query) in [
        (ExpressionKind::Emoji, "emoji.mwe", "禁止车"),
        (ExpressionKind::Kaomoji, "kaomoji.mwe", "开心"),
    ] {
        let start = Instant::now();
        let library = ExpressionLibrary::load(&root.join("expressions").join(file), kind)?;
        let load_ms = start.elapsed().as_secs_f64() * 1000.0;
        let words = vec![query.to_owned()];
        for _ in 0..20 {
            black_box(library.search(query, &words));
        }
        let mut times = Vec::new();
        let mut count = 0;
        for _ in 0..200 {
            let start = Instant::now();
            let result = black_box(library.search(black_box(query), &words));
            times.push(start.elapsed().as_secs_f64() * 1_000_000.0);
            count = result.matches.len();
        }
        searches.push(json!({"resource":file, "query":query, "load_ms":load_ms,
            "entries":library.document().entries.len(), "results":count,
            "search": distribution(times)}));
    }
    let dictionary = Dictionary::from_entries(
        vec![Entry {
            code: "jjaa".into(),
            word: "基线".into(),
            weight: 100,
        }],
        None,
        LoadOptions::default(),
    )?;
    let mut input = Vec::new();
    for n in 0..1020 {
        let mut engine = StateMachine::new(Arc::clone(&dictionary));
        let start = Instant::now();
        for c in "jjaad".chars() {
            black_box(engine.handle(InputEvent::Char(c)));
        }
        if n >= 20 {
            input.push(start.elapsed().as_secs_f64() * 1_000_000.0);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope":"old core path only; excludes JNI/UI/host; first load uses OS file cache",
            "warmup":20, "searches": searches, "input_jjaad":distribution(input)
        }))?
    );
    Ok(())
}
