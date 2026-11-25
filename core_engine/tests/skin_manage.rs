use core_engine::skin::{
    editor::Draft,
    manage::{execute, Request},
    preferences::Selection,
    Package,
};

#[test]
fn mobile_catalog_roundtrip_keeps_other_platform_and_does_not_activate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "original config").unwrap();
    let mut draft = Draft::builtin("jade").unwrap();
    draft
        .package
        .metadata
        .extensions
        .insert("future_root".into(), "preserved".into());
    draft
        .package
        .files
        .insert("shared/assets/proof.bin".into(), vec![1, 2, 3, 4]);
    let source = dir.path().join("source.mywubi-skin");
    draft.package.export_new(&source).unwrap();
    let preview = execute(Request::Inspect {
        source: source.clone(),
        platform: "android".into(),
    })
    .unwrap();
    assert_eq!(preview["name"], draft.package.metadata.name);
    assert!(!core_engine::skin::runtime::store_path(&path).exists());
    execute(Request::Import {
        path: path.clone(),
        source,
        platform: "android".into(),
    })
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original config");
    let items = execute(Request::List {
        path: path.clone(),
        platform: "android".into(),
    })
    .unwrap();
    assert_eq!(items["items"][0]["components"][0]["enabled"], true);
    let output = dir.path().join("export.mywubi-skin");
    execute(Request::Export {
        path,
        selection: Selection {
            id: draft.package.metadata.id.clone(),
            version: draft.package.metadata.version.clone(),
            component: "appearance".into(),
        },
        target: output.clone(),
    })
    .unwrap();
    assert_eq!(
        Package::open(&output).unwrap().encoded().unwrap(),
        draft.package.encoded().unwrap()
    );
}

#[test]
fn windows_only_is_importable_but_cannot_be_selected_on_android() {
    let dir = tempfile::tempdir().unwrap();
    let mut draft = Draft::builtin("plain").unwrap();
    draft.remove_platform("android").unwrap();
    let source = dir.path().join("windows.mywubi-skin");
    draft.package.export_new(&source).unwrap();
    let result = execute(Request::Import {
        path: dir.path().join("config.toml"),
        source,
        platform: "android".into(),
    })
    .unwrap();
    assert_eq!(result["item"]["components"].as_array().unwrap().len(), 0);
    assert!(result["item"]["reason"]
        .as_str()
        .unwrap()
        .contains("android"));
}

#[test]
fn delete_protects_active_skin_and_keyboard_then_removes_only_the_selected_version() {
    use core_engine::skin::{combination::Choice, runtime::store_path};
    use core_engine::Config;
    use std::fs;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let library = store_path(&path);
    fs::create_dir_all(&library).unwrap();
    let selected = Selection {
        id: "test.skin".into(),
        version: "1".into(),
        component: "appearance".into(),
    };
    let target = library.join("test.skin@1.mywubi-skin");
    let neighbor = library.join("test.skin@2.mywubi-skin");
    fs::write(&target, b"first").unwrap();
    fs::write(&neighbor, b"second").unwrap();
    let catalog = execute(Request::List {
        path: path.clone(),
        platform: "android".into(),
    })
    .unwrap();
    assert_eq!(catalog["items"][0]["unreadable"], true);
    assert_eq!(catalog["items"][0]["id"], selected.id);
    assert_eq!(catalog["items"][0]["version"], selected.version);
    let remove = || {
        execute(Request::Delete {
            path: path.clone(),
            id: selected.id.clone(),
            version: selected.version.clone(),
        })
    };
    let mut config = Config::default();
    config.skins.android.selection = Some(selected.clone());
    fs::write(&path, config.to_string_toml().unwrap()).unwrap();
    assert!(remove().unwrap_err().to_string().contains("正在使用"));
    config.skins.android.selection = None;
    fs::write(&path, config.to_string_toml().unwrap()).unwrap();
    let receipt = path.with_file_name("keyboard-selection-English.json");
    let choice = Choice {
        layout: selected.clone(),
        appearance: Default::default(),
        mapping: None,
    };
    fs::write(
        &receipt,
        serde_json::json!({"active":choice, "digest":"test", "previous":null}).to_string(),
    )
    .unwrap();
    assert!(remove().unwrap_err().to_string().contains("英文"));
    assert!(target.is_file());
    fs::remove_file(receipt).unwrap();
    assert_eq!(remove().unwrap()["deleted"], true);
    assert!(!target.exists());
    assert_eq!(fs::read(neighbor).unwrap(), b"second");
}
