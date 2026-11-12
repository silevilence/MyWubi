use core_engine::{expression_store::*, expressions::*, Config};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mywubi-expression-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.dict"), "a\t开心\n").unwrap();
        let mut config = Config::default();
        config.dictionary.system_table = "main.dict".into();
        config.save(root.join("config.toml")).unwrap();
        Self(root)
    }
    fn config(&self) -> PathBuf {
        self.0.join("config.toml")
    }
    fn source(&self, kind: ExpressionKind) -> PathBuf {
        let path = self.0.join(format!("{}.mwe", kind.slug()));
        let document = json!({
            "kind": kind, "name": "自定义合集", "version": "v1",
            "sources": [{"id":"fixture","url":"local:test","revision":"pinned","license":"MIT","notice":"Full test notice"}],
            "entries": [{"text":"👩🏽‍💻　ö\\_", "name":"开心", "triggers":["开心"], "order":0, "sources":["fixture"]}]
        });
        std::fs::write(&path, format!("{MAGIC}\n{document}")).unwrap();
        path
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn import_is_durable_unique_and_does_not_activate_or_modify_the_source() {
    let f = Files::new();
    let source = f.source(ExpressionKind::Emoji);
    let original = std::fs::read(&source).unwrap();
    let config = std::fs::read(f.config()).unwrap();
    let first = import(&f.config(), ExpressionKind::Emoji, &source).unwrap();
    let second = import(&f.config(), ExpressionKind::Emoji, &source).unwrap();
    assert_ne!(first.path, second.path);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(std::fs::read(f.config()).unwrap(), config);
    std::fs::remove_file(source).unwrap();
    let loaded = ExpressionLibrary::load(&first.path, ExpressionKind::Emoji).unwrap();
    assert_eq!(loaded.document().entries[0].text, "👩🏽‍💻　ö\\_");
    assert_eq!(first.sources[0].notice, "Full test notice");
    let choices = list(
        &f.config(),
        ExpressionKind::Emoji,
        &ExpressionSelection::default(),
    )
    .unwrap();
    assert_eq!(choices.len(), 3); // Missing builtin remains visible with its error.
    assert!(choices[0].builtin && choices[0].error.is_some());
    assert_eq!(choices.iter().filter(|c| c.error.is_none()).count(), 2);
}

#[test]
fn invalid_and_wrong_kind_imports_leave_active_configuration_and_catalog_unchanged() {
    let f = Files::new();
    let source = f.source(ExpressionKind::Emoji);
    assert!(import(&f.config(), ExpressionKind::Kaomoji, &source).is_err());
    std::fs::write(&source, "普通码表").unwrap();
    assert!(import(&f.config(), ExpressionKind::Emoji, &source).is_err());
    assert!(!f.0.join("expressions/imports").exists());
    let config = Config::load(f.config()).unwrap();
    assert_eq!(config.expressions, ExpressionsConfig::default());
}

#[test]
fn selected_external_or_broken_resource_is_listed_and_never_hidden_by_imports() {
    let f = Files::new();
    let source = f.source(ExpressionKind::Emoji);
    let selected = ExpressionSelection {
        resource: Some(source.clone()),
        ..Default::default()
    };
    let choices = list(&f.config(), ExpressionKind::Emoji, &selected).unwrap();
    assert!(choices
        .iter()
        .any(|c| c.path == source && c.entries == 1 && c.error.is_none()));
    std::fs::write(&source, "损坏").unwrap();
    let choices = list(&f.config(), ExpressionKind::Emoji, &selected).unwrap();
    assert!(choices
        .iter()
        .any(|c| c.path == source && c.error.is_some()));
}

#[test]
fn missing_builtins_are_prepared_for_existing_config_and_upgrades_preserve_user_data() {
    let f = Files::new();
    let application = f.0.join("app");
    let bundle = application.join("expressions");
    std::fs::create_dir_all(&bundle).unwrap();
    for kind in [ExpressionKind::Emoji, ExpressionKind::Kaomoji] {
        let source = f.source(kind);
        std::fs::copy(&source, bundle.join(source.file_name().unwrap())).unwrap();
    }
    let before = std::fs::read(f.config()).unwrap();
    let resolved = core_engine::config_path::resolve_config_path_from(&application, &f.0).unwrap();
    assert_eq!(resolved.path, f.config());
    let installed = f.0.join("expressions/emoji.mwe");
    assert_eq!(
        ExpressionLibrary::load(&installed, ExpressionKind::Emoji)
            .unwrap()
            .document()
            .entries
            .len(),
        1
    );
    std::fs::write(&installed, "user-edited resource").unwrap();
    core_engine::config_path::resolve_config_path_from(&application, &f.0).unwrap();
    assert_eq!(
        std::fs::read_to_string(&installed).unwrap(),
        "user-edited resource"
    );
    assert_eq!(std::fs::read(f.config()).unwrap(), before);
}

#[test]
fn selection_survives_reopen_and_main_table_change_and_restore_uses_builtin() {
    let f = Files::new();
    let source = f.source(ExpressionKind::Emoji);
    let custom = import(&f.config(), ExpressionKind::Emoji, &source).unwrap();
    prepare_builtin(&f.config(), &f.0, ExpressionKind::Emoji).unwrap();
    let mut config = Config::load(f.config()).unwrap();
    config.expressions.emoji.append = true;
    config.expressions.emoji.resource = Some(custom.path.strip_prefix(&f.0).unwrap().to_owned());
    config.save(f.config()).unwrap();
    let mut reopened = Config::load(f.config()).unwrap();
    assert_eq!(config.expressions, reopened.expressions);
    reopened.dictionary.system_table = "other.dict".into();
    reopened.save(f.config()).unwrap();
    assert_eq!(
        Config::load(f.config()).unwrap().expressions,
        config.expressions
    );
    reopened.expressions.emoji.resource = None;
    reopened.save(f.config()).unwrap();
    assert!(custom.path.is_file());
    assert_eq!(
        Config::load(f.config()).unwrap().expressions.emoji.resource,
        None
    );
}

fn call(request: Value) -> Value {
    serde_json::from_slice(&core_engine::ffi::call(
        &serde_json::to_vec(&json!({"version":1,"request":request})).unwrap(),
    ))
    .unwrap()
}
#[test]
fn json_facade_returns_metadata_and_imports_with_shared_validation() {
    let f = Files::new();
    let source = f.source(ExpressionKind::Emoji);
    let imported =
        call(json!({"op":"ImportExpression","path":f.config(),"kind":"emoji","source":source}));
    assert_eq!(imported["ok"], true, "{imported}");
    assert!(Path::new(imported["value"]["path"].as_str().unwrap()).is_file());
    let listed =
        call(json!({"op":"ListExpressions","path":f.config(),"kind":"emoji","selected":{}}));
    assert_eq!(listed["ok"], true, "{listed}");
    assert_eq!(listed["value"].as_array().unwrap().len(), 2);
    let invalid =
        call(json!({"op":"ImportExpression","path":f.config(),"kind":"kaomoji","source":source}));
    assert_eq!(invalid["ok"], false);
}

#[test]
fn all_six_categories_import_through_the_shared_protocol_without_changing_selection() {
    let f = Files::new();
    let before = std::fs::read(f.config()).unwrap();
    for kind in ExpressionKind::ALL {
        let source = f.source(kind);
        let imported = call(json!({"op":"ImportExpression", "path":f.config(),
            "kind":kind, "source":source}));
        assert_eq!(imported["ok"], true, "{imported}");
        let path = Path::new(imported["value"]["path"].as_str().unwrap());
        assert_eq!(
            ExpressionLibrary::load(path, kind).unwrap().document().kind,
            kind
        );
        let listed = call(json!({"op":"ListExpressions", "path":f.config(),
            "kind":kind, "selected":{"resource":path}}));
        assert_eq!(listed["ok"], true, "{listed}");
        assert!(listed["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["path"] == path.to_str().unwrap()));
        assert_eq!(std::fs::read(f.config()).unwrap(), before);
    }
}
