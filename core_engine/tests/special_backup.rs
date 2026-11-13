use core_engine::{
    expressions::{ExpressionKind, ExpressionLibrary},
    shortcuts::{availability, ShortcutEnvironment, ShortcutKind, ShortcutRule},
    special_backup, Config,
};
use serde_json::{json, Value};
use std::path::PathBuf;

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mywubi-special-backup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn path(&self) -> PathBuf {
        self.0.join("config.toml")
    }
    fn config(&self) -> Config {
        let mut config = Config::default();
        let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        for kind in ExpressionKind::ALL {
            let selected = config.expressions.selection_mut(kind);
            selected.resource = Some(bundle.join(kind.default_path()));
            selected.append = kind == ExpressionKind::Math;
            selected.search = kind == ExpressionKind::Kana;
        }
        config.shortcuts.rules = vec![
            ShortcutRule {
                code: "JJAD".into(),
                position: 2,
                kind: ShortcutKind::DateTime,
                format: "yyyy年MM月dd日".into(),
                enabled: true,
            },
            ShortcutRule {
                code: "jjad".into(),
                position: 3,
                kind: ShortcutKind::DateTime,
                format: "yyyy-MM-dd".into(),
                enabled: false,
            },
        ];
        config.save(self.path()).unwrap();
        config
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn portable_backup_preserves_resources_flags_rules_and_environment_independent_intent() {
    let f = Files::new();
    let config = f.config();
    let archive = f.0.join("backup.mwsi");
    special_backup::export(&f.path(), &config, &archive).unwrap();
    let destination = f.0.join("restored");
    std::fs::create_dir(&destination).unwrap();
    let path = destination.join("config.toml");
    let mut current = Config::default();
    current.dictionary.system_table = "another.dict".into();
    current.basic.candidate_count = 9;
    current.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let restored = special_backup::restore(&path, &current, &archive).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "restore only returns a draft"
    );
    assert_eq!(
        restored.dictionary.system_table,
        current.dictionary.system_table
    );
    assert_eq!(restored.basic.candidate_count, 9);
    assert_eq!(restored.shortcuts.rules[0].code, "jjad");
    assert!(restored.shortcuts.rules[0].enabled);
    assert!(!restored.shortcuts.rules[1].enabled);
    let conflict = ShortcutEnvironment {
        wildcard: Some('J'),
        auxiliary_entry: None,
    };
    assert!(!availability(&restored.shortcuts.rules[0], conflict).effective);
    assert!(availability(&restored.shortcuts.rules[0], ShortcutEnvironment::default()).effective);
    for kind in ExpressionKind::ALL {
        let original = config.expressions.selection(kind);
        let selected = restored.expressions.selection(kind);
        assert_eq!(
            (selected.append, selected.search),
            (original.append, original.search)
        );
        assert!(!selected.resource.as_ref().unwrap().is_absolute());
        let original = ExpressionLibrary::load(&original.path(&f.path(), kind), kind).unwrap();
        let imported = ExpressionLibrary::load(&selected.path(&path, kind), kind).unwrap();
        assert_eq!(original.document(), imported.document(), "{kind:?}");
    }
    restored.save(&path).unwrap();
    // Moving the directory retains every restored resource reference.
    let moved = f.0.join("moved");
    std::fs::rename(&destination, &moved).unwrap();
    let path = moved.join("config.toml");
    let restored = Config::load(&path).unwrap();
    for kind in ExpressionKind::ALL {
        ExpressionLibrary::load(
            &restored.expressions.selection(kind).path(&path, kind),
            kind,
        )
        .unwrap();
    }
}

#[test]
fn invalid_archives_and_missing_resources_do_not_change_config_or_replace_backup() {
    let f = Files::new();
    let mut config = f.config();
    let archive = f.0.join("backup.mwsi");
    special_backup::export(&f.path(), &config, &archive).unwrap();
    let valid = std::fs::read(&archive).unwrap();
    let before = std::fs::read(f.path()).unwrap();
    config.expressions.kana.resource = Some("missing.mwe".into());
    assert!(special_backup::export(&f.path(), &config, &archive)
        .unwrap_err()
        .contains("假名"));
    assert_eq!(std::fs::read(&archive).unwrap(), valid);
    for fault in ["missing", "duplicate", "wrong-kind", "bad-rule", "future"] {
        let mut value: Value = serde_json::from_slice(&valid).unwrap();
        match fault {
            "missing" => {
                value["resources"].as_array_mut().unwrap().pop();
            }
            "duplicate" => value["resources"][5] = value["resources"][0].clone(),
            "wrong-kind" => {
                value["resources"][5]["library"] = value["resources"][0]["library"].clone()
            }
            "bad-rule" => value["shortcuts"]["rules"][1]["position"] = json!(2),
            "future" => value["magic"] = json!("MYWUBI-SPECIAL-INPUT-2"),
            _ => unreachable!(),
        }
        std::fs::write(&archive, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            special_backup::restore(&f.path(), &config, &archive).is_err(),
            "{fault}"
        );
        assert_eq!(std::fs::read(f.path()).unwrap(), before);
        assert!(
            !f.0.join("expressions/imports").exists(),
            "validate all before importing: {fault}"
        );
    }
}

#[test]
fn facade_returns_staged_draft_and_export_receipt_without_saving_configuration() {
    let f = Files::new();
    let config = f.config();
    let archive = f.0.join("backup.mwsi");
    let invoke = |request| -> Value {
        serde_json::from_slice(&core_engine::ffi::call(
            &serde_json::to_vec(&json!({"version":1,"request":request})).unwrap(),
        ))
        .unwrap()
    };
    let exported =
        invoke(json!({"op":"ExportSpecialInput","path":f.path(),"config":config,"target":archive}));
    assert_eq!(exported["ok"], true, "{exported}");
    assert_eq!(exported["value"]["exported"], true);
    let before = std::fs::read(f.path()).unwrap();
    let restored = invoke(
        json!({"op":"RestoreSpecialInput","path":f.path(),"config":config,"source":archive}),
    );
    assert_eq!(restored["ok"], true, "{restored}");
    assert_eq!(
        restored["value"]["config"]["shortcuts"]["rules"][0]["code"],
        "jjad"
    );
    assert_eq!(std::fs::read(f.path()).unwrap(), before);
}
