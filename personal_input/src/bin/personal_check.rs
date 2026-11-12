//! Reproducible synthetic temporal comparison and optional sustained core replay.
use personal_input::{store::Store, *};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "export" || s == "import") {
        let root = args
            .get(2)
            .ok_or(Error::Invalid("missing store directory"))?;
        let path = args.get(3).ok_or(Error::Invalid("missing package path"))?;
        let store = Store::new(root);
        let password = args.get(4).map(String::as_str);
        if args[1] == "export" {
            core_engine::data::atomic_write(
                std::path::Path::new(path),
                &exchange::export(&store, password)?,
            )?;
        } else {
            let events = exchange::decode(&std::fs::read(path)?, password)?;
            let preview = exchange::preview(&store, &events, exchange::ImportMode::Merge)?;
            exchange::import(
                &store,
                events,
                exchange::ImportMode::Merge,
                &preview.revision,
                128,
            )?;
            println!("{}", serde_json::to_string(&preview)?);
        }
        return Ok(());
    }
    let root = args
        .get(1)
        .ok_or(Error::Invalid("usage: personal_check OUTPUT_DIR [seconds]"))?;
    let seconds = args.get(2).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let store = Store::new(root);
    std::fs::create_dir_all(&store.root)?;
    if store.stats()?.records != 0 {
        return Err(Error::Invalid("验收输出目录必须没有个人记录"));
    }
    let source = identity()?;
    let begin = now_ms();
    let events: Vec<_> = (1..=100)
        .map(|n| Event {
            format: FORMAT,
            id: format!("{source}:{n}"),
            source: source.clone(),
            session: "synthetic".into(),
            sequence: n,
            time_ms: begin + n,
            mode: "main".into(),
            evidence: Evidence::Observed,
            before: Context {
                left: Some(if n % 2 == 0 { "明天去" } else { "下班回" }.into()),
                right: Some("".into()),
                selected: Some("".into()),
                ..Default::default()
            },
            after: None,
            change: Change::Commit {
                text: if n % 2 == 0 { "图书馆" } else { "实验室" }.into(),
                source: Source::Code,
                method: "replay".into(),
                outcome: Outcome::Confirmed,
                request: None,
                candidate: None,
            },
            extensions: Default::default(),
        })
        .collect();
    store.append(
        events.clone(),
        0,
        &PersonalConfig {
            collect: true,
            ..Default::default()
        },
    )?;
    let samples = samples::build(events);
    let comparison = learning::compare(
        &samples.kept,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(2),
    )?;
    let start = Instant::now();
    let model = learning::train(&store, &AtomicBool::new(false))?;
    let train_ms = start.elapsed().as_millis();
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    let background_store = store.clone();
    let template = store
        .snapshot()?
        .events
        .into_values()
        .next()
        .ok_or(Error::Invalid("missing fixture"))?;
    let training = std::thread::spawn(move || {
        let mut sequence = 101u64;
        let mut rounds = 0u64;
        while !signal.load(Ordering::Acquire) {
            if rounds.is_multiple_of(30) {
                let events = (0..20)
                    .map(|_| {
                        let mut e = template.clone();
                        e.sequence = sequence;
                        e.id = format!("{}:{sequence}", e.source);
                        e.time_ms = now_ms() + sequence;
                        sequence += 1;
                        e
                    })
                    .collect();
                let _ = background_store.append(
                    events,
                    0,
                    &PersonalConfig {
                        collect: true,
                        ..Default::default()
                    },
                );
            }
            let _ = learning::train(&background_store, &signal);
            rounds += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    let mut config = core_engine::Config::default();
    config.basic.auto_commit_unique = false;
    let runtime = core_engine::runtime::RuntimeSnapshot {
        dict: core_engine::Dictionary::from_text("a\t测\n")
            .map_err(|_| Error::Invalid("fixture invalid"))?,
        config,
        revision: 1,
        expressions: Arc::default(),
        temporary: Arc::default(),
        config_path: store.root.join("config.toml"),
        system_table_path: "s".into(),
        user_table_path: "u".into(),
    };
    let mut session = core_engine::session::Session::new(
        &runtime,
        core_engine::BehaviorProfile::TouchV1,
        core_engine::session::HostContext::default(),
    );
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut timings = Vec::new();
    let mut predictions = 0usize;
    loop {
        let start = Instant::now();
        for action in [
            core_engine::Action::InputCode('a'),
            core_engine::Action::Space,
        ] {
            let plan = session
                .prepare(action)
                .map_err(|_| Error::Invalid("input replay failed"))?;
            session.acknowledge(plan.token, core_engine::session::HostOutcome::Applied);
        }
        timings.push(start.elapsed().as_micros() as u64);
        predictions += usize::from(
            model
                .predict("明天去", "", 3)
                .first()
                .is_some_and(|s| s == "图书馆"),
        );
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    stop.store(true, Ordering::Release);
    let _ = training.join();
    timings.sort_unstable();
    let percentile = |n: usize| timings[(timings.len() - 1) * n / 100];
    let report = serde_json::json!({"synthetic":true,"quality_accepted":false,"comparison":comparison,"train_ms":train_ms,"model_bytes":std::fs::metadata(store.root.join("model.json"))?.len(),"duration_seconds":seconds,"input_rounds":timings.len(),"input_pair_us":{"p50":percentile(50),"p95":percentile(95),"p99":percentile(99)},"prediction_hits":predictions});
    core_engine::data::atomic_write(
        &store.root.join("report.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", report);
    Ok(())
}
