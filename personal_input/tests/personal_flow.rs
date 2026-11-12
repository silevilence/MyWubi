use personal_input::{
    exchange::{self, ImportMode},
    store::Store,
    *,
};
use std::sync::atomic::AtomicBool;

fn event(n: u64) -> Event {
    let source = "0123456789abcdef0123456789abcdef".to_owned();
    Event {
        format: FORMAT,
        id: format!("{source}:{n}"),
        source,
        session: "editor-a".into(),
        sequence: n,
        time_ms: now_ms() + n,
        mode: "main".into(),
        evidence: Evidence::Observed,
        before: Context {
            left: Some("明天去".into()),
            right: Some("".into()),
            selected: Some("".into()),
            ..Default::default()
        },
        after: None,
        change: Change::Commit {
            text: "图书馆".into(),
            source: Source::Code,
            method: "space".into(),
            outcome: Outcome::Confirmed,
            request: None,
            candidate: None,
        },
        extensions: Default::default(),
    }
}
fn config() -> PersonalConfig {
    PersonalConfig {
        collect: true,
        ..Default::default()
    }
}
fn fill(store: &Store, count: u64) {
    store
        .append((1..=count).map(event).collect(), 0, &config())
        .unwrap();
}

#[test]
fn exchange_roundtrips_encrypted_and_plaintext_without_changing_identity() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let a = Store::new(a.path());
    let b = Store::new(b.path());
    fill(&a, 30);
    for password in [None, Some("离线交换密码")] {
        let bytes = exchange::export(&a, password).unwrap();
        let events = exchange::decode(&bytes, password).unwrap();
        let preview = exchange::preview(&b, &events, ImportMode::Merge).unwrap();
        exchange::import(&b, events, ImportMode::Merge, &preview.revision, 64).unwrap();
        assert_eq!(a.snapshot().unwrap().events, b.snapshot().unwrap().events);
    }
    let back = exchange::decode(&exchange::export(&b, None).unwrap(), None).unwrap();
    let preview = exchange::preview(&a, &back, ImportMode::Merge).unwrap();
    assert_eq!(
        (preview.added, preview.duplicates, preview.conflicts),
        (0, 30, 0)
    );
}
#[test]
fn export_preserves_edit_identity_after_referenced_record_is_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let mut old = event(1);
    old.time_ms = 1;
    let mut correction = event(2);
    correction.change = Change::Edit {
        operation: EditKind::Delete,
        related: Some(old.id.clone()),
    };
    store
        .append(vec![old, correction.clone()], 0, &config())
        .unwrap();
    let events = exchange::decode(&exchange::export(&store, None).unwrap(), None).unwrap();
    assert_eq!(events, vec![correction]);
}
#[test]
fn corrupt_wrong_password_and_future_version_packages_do_not_mutate_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    fill(&store, 2);
    let bytes = exchange::export(&store, Some("correct")).unwrap();
    assert!(exchange::decode(&bytes, Some("wrong")).is_err());
    assert!(exchange::decode(&bytes[..bytes.len() - 1], Some("correct")).is_err());
    let mut future = bytes.clone();
    future[7] = b'2';
    assert!(exchange::decode(&future, None).is_err());
    assert_eq!(store.stats().unwrap().records, 2);
}
#[test]
fn conflicts_and_stale_previews_preserve_new_input() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    fill(&store, 2);
    let mut conflict = store
        .snapshot()
        .unwrap()
        .events
        .values()
        .next()
        .unwrap()
        .clone();
    conflict.mode = "different".into();
    let preview = exchange::preview(&store, &[conflict.clone()], ImportMode::Merge).unwrap();
    assert_eq!(preview.conflicts, 1);
    assert!(exchange::import(
        &store,
        vec![conflict],
        ImportMode::Merge,
        &preview.revision,
        64
    )
    .is_err());
    let incoming = vec![event(10)];
    let preview = exchange::preview(&store, &incoming, ImportMode::Replace).unwrap();
    store.append(vec![event(3)], 0, &config()).unwrap();
    assert!(
        exchange::import(&store, incoming, ImportMode::Replace, &preview.revision, 64).is_err()
    );
    assert_eq!(store.stats().unwrap().records, 3);
}
#[test]
fn concurrent_writers_and_truncated_tail_recover_with_a_gap() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    std::thread::scope(|scope| {
        for n in 1..=8 {
            let store = store.clone();
            scope.spawn(move || store.append(vec![event(n)], 0, &config()).unwrap());
        }
    });
    std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join("journal-0.jsonl"))
        .unwrap()
        .write_all(b"{partial")
        .unwrap();
    assert_eq!(store.stats().unwrap().gaps, 1);
    store.append(vec![event(9)], 0, &config()).unwrap();
    let snapshot = Store::new(dir.path()).snapshot().unwrap();
    assert_eq!(snapshot.events.len(), 9);
    assert_eq!(snapshot.gaps, 1);
}
#[test]
fn retention_keeps_bounded_identity_tombstones() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let mut old = event(1);
    old.time_ms = 1;
    store.append(vec![old.clone()], 0, &config()).unwrap();
    assert_eq!(store.stats().unwrap().records, 0);
    let preview = exchange::preview(&store, &[old.clone()], ImportMode::Merge).unwrap();
    assert_eq!(preview.duplicates, 1);
    exchange::import(&store, vec![old], ImportMode::Merge, &preview.revision, 64).unwrap();
    assert_eq!(store.stats().unwrap().records, 0);
}
#[test]
fn samples_keep_original_context_and_exclude_failed_unselected_and_corrected_commits() {
    let one = event(1);
    let mut two = event(2);
    two.before.right = Some("出差".into());
    two.before.selected = Some("旧词".into());
    let mut failed = event(3);
    if let Change::Commit { outcome, .. } = &mut failed.change {
        *outcome = Outcome::CommandAccepted;
    }
    let mut correction = event(4);
    correction.change = Change::Edit {
        operation: EditKind::Delete,
        related: Some(one.id.clone()),
    };
    let mut unknown = event(5);
    unknown.before.left = None;
    let samples = samples::build(vec![two.clone(), failed, correction, one, unknown]);
    assert_eq!(samples.kept.len(), 1);
    assert_eq!(samples.kept[0].event, two.id);
    assert_eq!(samples.kept[0].left, "明天去");
    assert_eq!(samples.kept[0].right, "出差");
    assert_eq!(samples.kept[0].edit, "replace");
    assert_eq!(samples.excluded.len(), 3);
}
#[test]
fn first_train_repeat_import_update_rollback_and_clear_are_traceable() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    fill(&store, 40);
    let cancel = AtomicBool::new(false);
    let first = learning::train(&store, &cancel).unwrap();
    assert_eq!(first.predict("明天去", "", 3), vec!["图书馆"]);
    assert!(first.predict("明天去", "图书馆", 3).is_empty());
    let same = learning::train(&store, &cancel).unwrap();
    assert_eq!(same.version, first.version);
    store
        .append((41..=60).map(event).collect(), 0, &config())
        .unwrap();
    let next = learning::train(&store, &cancel).unwrap();
    assert_eq!(next.sample_count, 60);
    assert_ne!(next.version, first.version);
    learning::rollback(&store).unwrap();
    assert_eq!(learning::load(&store).unwrap().version, first.version);
    store.clear(false).unwrap();
    assert_eq!(learning::load(&store).unwrap().version, first.version);
    store.clear(true).unwrap();
    assert!(learning::load(&store).is_err());
}
#[test]
fn replacement_correction_and_cancel_cannot_publish_or_reuse_stale_knowledge() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    fill(&store, 40);
    let cancel = AtomicBool::new(false);
    let first = learning::train(&store, &cancel).unwrap();
    store
        .append((41..=60).map(event).collect(), 0, &config())
        .unwrap();
    assert!(learning::train(&store, &AtomicBool::new(true)).is_err());
    assert_eq!(learning::load(&store).unwrap().version, first.version);
    let mut corrected = event(61);
    corrected.change = Change::Edit {
        operation: EditKind::Replace,
        related: Some(event(1).id),
    };
    store.append(vec![corrected], 0, &config()).unwrap();
    assert!(learning::load(&store).is_err());
    let incoming = vec![event(100)];
    let preview = exchange::preview(&store, &incoming, ImportMode::Replace).unwrap();
    exchange::import(&store, incoming, ImportMode::Replace, &preview.revision, 64).unwrap();
    assert!(learning::load(&store).is_err());
    assert!(learning::train(&store, &cancel).is_err());
}
#[test]
fn disabled_sensitive_and_revoked_queue_never_write_text() {
    use fs2::FileExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut cfg = core_engine::Config::default();
    cfg.personal.collect = true;
    cfg.save(&path).unwrap();
    let store = Store::beside_config(&path);
    std::fs::create_dir_all(&store.root).unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(store.root.join("store.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let mut recorder = recorder::Recorder::new(path).unwrap();
    let record = |r: &mut recorder::Recorder| {
        r.record(
            "main",
            Context::default(),
            None,
            Change::Preedit {
                text: "private-marker".into(),
            },
            Evidence::Observed,
        )
    };
    assert!(record(&mut recorder).is_none());
    recorder.configure(&cfg.personal, false, "password");
    assert!(record(&mut recorder).is_none());
    recorder.configure(&cfg.personal, true, "normal");
    assert!(record(&mut recorder).is_some());
    recorder.configure(&cfg.personal, false, "disabled");
    recorder.configure(&cfg.personal, true, "new-session");
    FileExt::unlock(&lock).unwrap();
    drop(recorder);
    assert_eq!(store.stats().unwrap().records, 0);
}
#[test]
fn every_switch_combination_roundtrips_without_implicit_consent() {
    for bits in 0..8 {
        let config = core_engine::Config {
            personal: PersonalConfig {
                collect: bits & 1 != 0,
                learn: bits & 2 != 0,
                predict: bits & 4 != 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let json = serde_json::to_vec(&config).unwrap();
        let saved: core_engine::Config = serde_json::from_slice(&json).unwrap();
        assert_eq!(saved.personal, config.personal);
    }
    let old: core_engine::Config = serde_json::from_str("{}").unwrap();
    assert_eq!(old.personal, PersonalConfig::default());
}

#[test]
fn normal_shutdown_drains_authorized_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut cfg = core_engine::Config::default();
    cfg.personal.collect = true;
    cfg.save(&path).unwrap();
    let mut recorder = recorder::Recorder::new(path.clone()).unwrap();
    recorder.configure(&cfg.personal, true, "normal");
    let e = event(1);
    recorder.record("main", e.before, None, e.change, Evidence::Observed);
    drop(recorder);
    assert_eq!(Store::beside_config(&path).stats().unwrap().records, 1);
}
