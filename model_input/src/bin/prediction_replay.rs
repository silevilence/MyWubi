//! Authored regression replay through actual session commits, not a quality benchmark.
use core_engine::{
    runtime::RuntimeSnapshot, session::*, Action, BehaviorProfile, Config, Dictionary,
};
use serde::Deserialize;
use std::{path::Path, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    prefix: String,
    acceptable: Vec<String>,
    forbidden: Vec<String>,
    stop: bool,
}

fn commit(session: &mut Session, action: Action) -> Result<(), Box<dyn std::error::Error>> {
    let edit = session
        .prepare(action)
        .map_err(|e| format!("prepare: {e:?}"))?;
    if session.acknowledge(edit.token, HostOutcome::Applied).status != AckStatus::Published {
        return Err("commit was not acknowledged".into());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../evaluation/prediction-regressions.json"))?;
    let mut config = Config::default();
    config.prediction.enabled = true;
    let runtime = RuntimeSnapshot {
        dict: Dictionary::load(root.join("tables/wubi86.dict"))?,
        config,
        revision: 1,
        temporary: Arc::default(),
        expressions: Arc::default(),
        config_path: "c".into(),
        system_table_path: "s".into(),
        user_table_path: "u".into(),
    };
    let mut rows = Vec::new();
    let mut failures = 0;
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        for case in &cases {
            let mut session = Session::new(&runtime, profile, HostContext::default());
            commit(&mut session, Action::InsertLiteral(case.prefix.clone()))?;
            let candidates: Vec<_> = session
                .candidate_window(0, 64)
                .candidates
                .into_iter()
                .map(|c| c.text)
                .collect();
            let recalled = case.acceptable.iter().any(|text| candidates.contains(text));
            let invalid = candidates.iter().any(|text| case.forbidden.contains(text))
                || (case.stop && !candidates.is_empty())
                || (!case.acceptable.is_empty() && !recalled);
            failures += usize::from(invalid);
            rows.push(serde_json::json!({"profile":format!("{profile:?}"),"prefix":case.prefix,"shown":candidates,"recalled":recalled,"top1":candidates.first().is_some_and(|c|case.acceptable.contains(c)),"stop_expected":case.stop,"invalid":invalid}));
        }
    }
    let mut chains = Vec::new();
    for seed in ["五", "北", "北京", "今天", "明天", "谢谢", "输入法", "分钟"] {
        let mut session = Session::new(&runtime, BehaviorProfile::LegacyV1, HostContext::default());
        commit(&mut session, Action::InsertLiteral(seed.into()))?;
        let mut text = seed.to_owned();
        let mut steps = Vec::new();
        for _ in 0..16 {
            let Some(first) = session.candidate_window(0, 1).candidates.into_iter().next() else {
                break;
            };
            text.push_str(&first.text);
            steps.push(first.text);
            commit(&mut session, Action::Space)?;
        }
        let stopped = !session.has_prediction();
        failures += usize::from(!stopped);
        chains.push(serde_json::json!({"seed":seed,"steps":steps,"text":text,"stopped":stopped}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"scope":"Authored previously inspected engineering cases; not independent quality or sentence fluency evidence", "rows":rows,"chains":chains,"failures":failures})
        )?
    );
    if failures != 0 {
        return Err("word-boundary regression failed".into());
    }
    Ok(())
}
