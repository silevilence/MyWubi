//! Reproducible, offline lookup baseline; this does not measure screen latency.
use core_engine::dictionary::{Dictionary, SearchOptions};
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut reports = Vec::new();
    for path in ["tables/wubi86.dict", "tables/pinyin.dict"] {
        let bytes = std::fs::read(path)?;
        let start = Instant::now();
        let dict = Dictionary::load(path)?;
        let load_ms = start.elapsed().as_secs_f64() * 1000.0;
        let codes: Vec<_> = dict
            .entries()
            .iter()
            .step_by((dict.len() / 100).max(1))
            .take(100)
            .map(|e| e.code.as_str())
            .collect();
        let mut samples = Vec::new();
        let mut checked = 0;
        for round in 0..12 {
            for code in &codes {
                let start = Instant::now();
                let entries = black_box(dict.search_literal(
                    black_box(code),
                    SearchOptions {
                        prefer_exact: true,
                        limit: 50,
                    },
                ));
                let elapsed = start.elapsed().as_secs_f64() * 1_000_000.0;
                if !entries.iter().any(|entry| entry.code == *code) {
                    return Err(format!("exact candidate missing: {path} {code}").into());
                }
                if round >= 2 {
                    samples.push(elapsed);
                    checked += 1;
                }
            }
        }
        samples.sort_by(f64::total_cmp);
        reports.push(serde_json::json!({"path":path,"bytes":bytes.len(),
            "crc32":format!("{:08x}", crc32fast::hash(&bytes)),"entries":dict.len(),
            "load_ms":load_ms,"count":checked,"accuracy":1.0,
            "p50_us":samples[samples.len()/2],"p95_us":samples[samples.len()*95/100],
            "p99_us":samples[samples.len()*99/100]}));
    }
    println!("{}", serde_json::to_string_pretty(&reports)?);
    Ok(())
}
