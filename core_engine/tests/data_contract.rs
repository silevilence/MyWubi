use core_engine::{config::ValidationContext, data::*, *};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "mywubi-data-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn conflict_detects_same_length_external_edit_and_leaves_draft_and_disk_intact() {
    let f = Files::new();
    let path = f.0.join("config.toml");
    let c = Config::default();
    c.save(&path).unwrap();
    let version = FileVersion::read(&path).unwrap();
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("font_size = 14", "font_size = 15");
    std::fs::write(&path, &text).unwrap();
    assert!(matches!(
        save_config(
            &c,
            &path,
            ValidationContext::TouchSave,
            WritePolicy::Expected(version)
        ),
        Err(DataError::Conflict(_))
    ));
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    assert_eq!(c.appearance.font_size, 14);
}

#[test]
fn config_failure_reports_table_saved_and_invalid_table_writes_nothing() {
    let f = Files::new();
    let table = f.0.join("main.dict");
    let blocked = f.0.join("config.toml");
    std::fs::create_dir(&blocked).unwrap();
    let c = Config::default();
    let header = TableConfig::default();
    let entries = vec![Entry {
        code: "a".into(),
        word: "甲".into(),
        weight: 1,
    }];
    let report = save_bundle(
        &c,
        &blocked,
        ValidationContext::DesktopSave,
        WritePolicy::LegacyOverwrite,
        Some(TableDraft {
            original: &table,
            target: &table,
            config: &header,
            entries: &entries,
            policy: WritePolicy::LegacyOverwrite,
        }),
    );
    assert_eq!(report.saved, [table.clone()]);
    assert!(report.failure.is_some());
    let old = std::fs::read(&table).unwrap();
    let invalid = vec![Entry {
        code: "!".into(),
        word: "坏".into(),
        weight: 1,
    }];
    assert!(core_engine::data::save_table(
        &table,
        &table,
        &header,
        &invalid,
        WritePolicy::LegacyOverwrite
    )
    .is_err());
    assert_eq!(std::fs::read(table).unwrap(), old);
}

#[test]
fn save_as_relocates_references_and_package_check_rejects_escape() {
    let f = Files::new();
    let main = f.0.join("main.dict");
    let py = f.0.join("py.dict");
    let dest = f.0.join("copy/main.dict");
    std::fs::write(&py, "---\ntype: pinyin\n---\nni\t你\n").unwrap();
    let mut header = TableConfig::default();
    header.temporary.pinyin_table = "py.dict".into();
    let entries = vec![Entry {
        code: "a".into(),
        word: "甲".into(),
        weight: 1,
    }];
    core_engine::data::save_table(
        &main,
        &main,
        &header,
        &entries,
        WritePolicy::LegacyOverwrite,
    )
    .unwrap();
    assert_eq!(validate_package(&f.0, &main).unwrap().len(), 2);
    let (relocated, _) = core_engine::data::save_table(
        &main,
        &dest,
        &header,
        &entries,
        WritePolicy::LegacyOverwrite,
    )
    .unwrap();
    assert_eq!(PathBuf::from(relocated.temporary.pinyin_table), py);
    assert!(validate_package(&f.0.join("copy"), &dest).is_err());
}

#[test]
fn user_import_overrides_weights_and_stale_identity_never_deletes_another_entry() {
    let f = Files::new();
    let path = f.0.join("user.dict");
    let source = f.0.join("import.dict");
    let initial = FileVersion::read(&path).unwrap();
    let result = edit_user(
        &path,
        WritePolicy::Expected(initial.clone()),
        UserEdit::Add(Entry {
            code: "a".into(),
            word: "甲".into(),
            weight: 1,
        }),
    )
    .unwrap();
    assert_eq!(result.added, 1);
    assert!(matches!(
        edit_user(
            &path,
            WritePolicy::Expected(initial),
            UserEdit::Remove(UserEntryId {
                code: "a".into(),
                word: "甲".into()
            })
        ),
        Err(DataError::Conflict(_))
    ));
    std::fs::write(&source, "a\t甲\t99\nb\t乙\t5\n").unwrap();
    let result = edit_user(
        &path,
        WritePolicy::Expected(result.version),
        UserEdit::Import(source),
    )
    .unwrap();
    assert_eq!((result.added, result.updated), (1, 1));
    let old = std::fs::read(&path).unwrap();
    assert!(edit_user(
        &path,
        WritePolicy::Expected(result.version),
        UserEdit::Remove(UserEntryId {
            code: "x".into(),
            word: "不存在".into()
        })
    )
    .is_err());
    assert_eq!(std::fs::read(path).unwrap(), old);
}
