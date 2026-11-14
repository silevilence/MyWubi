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

