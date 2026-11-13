//! SI-15 fixed-workload core measurements. No GUI, downloads or live IME installation.
use core_engine::{
    expressions::*,
    runtime::*,
    shortcut_time::{Clock, LocalTime},
    shortcuts::*,
    BehaviorProfile, Config, Dictionary, InputEvent,
};
use serde_json::{json, Value};
use std::{hint::black_box, path::PathBuf, sync::Arc, time::Instant};

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> Result<LocalTime, String> {
        Ok(core_engine::shortcut_preview::example_time())
    }
}
fn distribution(mut times: Vec<f64>) -> Value {
    times.sort_by(f64::total_cmp);
    let percentile = |p: usize| times[(times.len() * p).div_ceil(100) - 1];
    json!({"samples":times.len(),"p50_us":percentile(50),"p95_us":percentile(95),"p99_us":percentile(99)})
}
fn rules(value: &Value, enabled: bool) -> Vec<ShortcutRule> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| ShortcutRule {
            code: row["code"].as_str().unwrap().into(),
            position: row["position"].as_u64().unwrap() as u32,
            format: row["format"].as_str().unwrap().into(),
            kind: ShortcutKind::DateTime,
            enabled,
        })
        .collect()
}
fn synthetic(workload: &Value) -> ExpressionResources {
    let mut resources = ExpressionResources::default();
    let count = workload["entries_per_kind"].as_u64().unwrap() as usize;
    for value in workload["kinds"].as_array().unwrap() {
        let kind: ExpressionKind = serde_json::from_value(value.clone()).unwrap();
        let entries = (0..count)
            .map(|i| ExpressionEntry {
                text: format!("{}{i}", kind.slug()),
                name: format!("样例{i}"),
                triggers: vec![workload["trigger"].as_str().unwrap().into()],
                aliases: vec![],
                tags: vec![],
                order: i as u32,
                sources: vec!["synthetic".into()],
            })
            .collect();
        let library = ExpressionLibrary::new(
            ExpressionDocument {
                kind,
                name: kind.label().into(),
                version: "SI-01-fixed".into(),
                entries,
                sources: vec![ExpressionSource {
                    id: "synthetic".into(),
                    url: "local:SI-01".into(),
                    revision: "1".into(),
                    license: "MIT".into(),
                    notice: "Synthetic benchmark data".into(),
                }],
            },
            kind,
        )
        .unwrap();
        let resource = match kind {
            ExpressionKind::Emoji => &mut resources.emoji,
            ExpressionKind::Kaomoji => &mut resources.kaomoji,
            ExpressionKind::Kana => &mut resources.kana,
            ExpressionKind::Math => &mut resources.math,
            ExpressionKind::Punctuation => &mut resources.punctuation,
            ExpressionKind::Symbols => &mut resources.symbols,
        };
        resource.library = Some(Arc::new(library));
    }
    resources
}
fn snapshot(config: Config, resources: ExpressionResources) -> RuntimeSnapshot {
    RuntimeSnapshot {
        revision: 1,
        dict: Dictionary::from_text("jjaa\t基线\t100\n").unwrap(),
        temporary: Arc::default(),
        expressions: Arc::new(resources),
        config,
        config_path: "config.toml".into(),
        system_table_path: "synthetic.dict".into(),
        user_table_path: "user.dict".into(),
    }
}
fn measure(runtime: &RuntimeSnapshot, input: &str, samples: usize) -> Value {
    let mut times = Vec::with_capacity(samples);
    let mut count = 0;
    for n in 0..samples + 20 {
        let mut engine = runtime.state_machine();
        engine.set_clock(Arc::new(FixedClock));
        let start = Instant::now();
        for c in input.chars() {
            black_box(engine.handle(InputEvent::Char(black_box(c))));
        }
        black_box(engine.candidates());
        if n >= 20 {
            times.push(start.elapsed().as_secs_f64() * 1_000_000.0);
        }
        count = core_engine::state_machine::semantic::SemanticEngine::new(
            engine,
            BehaviorProfile::LegacyV1,
        )
        .candidate_window(0, 0)
        .total;
    }
    json!({"input":input,"total_candidates":count,"timing":distribution(times)})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let workloads: Value = serde_json::from_str(include_str!(
        "../../docs/validation/special-input/workloads.json"
    ))?;
    let mut result = json!({"scope":"Windows core only; excludes JNI, drawing and host; OS file cache retained","warmup":20});
    let empty = snapshot(Config::default(), ExpressionResources::default());
    result["disabled_empty"] = measure(&empty, "jjaad", 1000);
    drop(empty);
    let mut config = Config::default();
    config.shortcuts.rules = rules(&workloads["misses"]["rules"], false);
    result["disabled_1024"] = measure(
        &snapshot(config, ExpressionResources::default()),
        "jjaad",
        1000,
    );
    let mut config = Config::default();
    config.shortcuts.rules = rules(&workloads["misses"]["rules"], true);
    let misses = snapshot(config, ExpressionResources::default());
    result["misses_fixed"] = measure(
        &misses,
        workloads["misses"]["input"].as_str().unwrap(),
        1000,
    );
    result["misses_five_char"] = measure(&misses, "jjaad", 1000);
    drop(misses);
    let mut config = Config::default();
    config.basic.auto_commit_unique = false;
    for kind in ExpressionKind::ALL {
        config.expressions.selection_mut(kind).append = true;
    }
    let mut multi = snapshot(config, synthetic(&workloads["multi_category"]));
    multi.dict = Dictionary::from_text("jjad\t基线\t100\n").unwrap();
    result["multi_category"] = measure(&multi, "jjad", 200);
    // These are separate workloads, not simultaneously active runtime snapshots.
    drop(multi);
    let mut config = Config::default();
    config.shortcuts.rules = rules(&workloads["same_code"]["rules"], true);
    result["same_code_32"] = measure(
        &snapshot(config, ExpressionResources::default()),
        "jjad",
        200,
    );
    let mut config = Config::load(root.join("config.toml"))?;
    for kind in ExpressionKind::ALL {
        let selected = config.expressions.selection_mut(kind);
        selected.append = true;
        selected.search = true;
    }
    config.shortcuts.rules = rules(&workloads["misses"]["rules"], true);
    config.shortcuts.rules.truncate(992);
    config
        .shortcuts
        .rules
        .extend(rules(&workloads["same_code"]["rules"], true));
    drop(workloads);
    let start = Instant::now();
    let full = RuntimeBuilder::build(
        root.join("config.toml"),
        config,
        BehaviorProfile::LegacyV1,
        LoadPolicy::Startup,
    )
    .map_err(|e| e.message)?;
    let engine = full.state_machine();
    black_box(&engine);
    result["all_resources_build_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
    result["all_resources_entries"] = json!(ExpressionKind::ALL
        .into_iter()
        .map(|kind| {
            let resource = full.expressions.resource(kind);
            (
                kind.slug(),
                resource
                    .library
                    .as_ref()
                    .map(|l| l.document().entries.len())
                    .unwrap_or_default(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>());
    result["all_resources_rules"] = json!(full.config.shortcuts.rules.len());
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
