use core_engine::{
    group_input::Position,
    group_search::{query, SearchStatus, WORK_BUDGET},
    Dictionary,
};
use std::{hint::black_box, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tables/wubi86.dict".into());
    let dict = Dictionary::load(&path)?;
    let mut times = Vec::new();
    let mut incomplete = 0;
    let mut bytes = 0;
    for round in 0..12 {
        for entry in dict
            .entries()
            .iter()
            .step_by((dict.len() / 100).max(1))
            .take(100)
        {
            let positions: Vec<_> = entry
                .code
                .chars()
                .map(|c| Position {
                    characters: if c.is_ascii_lowercase() {
                        format!(
                            "{}{}",
                            c,
                            if c == 'z' { 'a' } else { (c as u8 + 1) as char }
                        )
                    } else {
                        c.to_string()
                    },
                    selected: None,
                })
                .collect();
            let start = Instant::now();
            let result = black_box(query(&dict, &positions, WORK_BUDGET));
            let elapsed = start.elapsed().as_secs_f64() * 1_000_000.0;
            if result.status == SearchStatus::Complete
                && !result.candidates.iter().any(|m| m.text == entry.word)
            {
                return Err("lost reference candidate".into());
            }
            if round >= 2 {
                times.push(elapsed);
                incomplete += usize::from(result.status != SearchStatus::Complete);
                bytes = bytes.max(
                    result
                        .candidates
                        .iter()
                        .map(|m| m.text.len() + m.codes.iter().map(String::len).sum::<usize>())
                        .sum::<usize>(),
                );
            }
        }
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"path":path,"entries":dict.len(),"count":times.len(),"incomplete":incomplete,"p50_us":times[times.len()/2],"p95_us":times[times.len()*95/100],"p99_us":times[times.len()*99/100],"peak_result_string_bytes":bytes,"extra_index_bytes":0})
        )?
    );
    Ok(())
}
