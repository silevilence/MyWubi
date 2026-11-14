use core_engine::skin::{editor::Draft, Package};

#[test]
fn editing_one_platform_preserves_other_platform_and_unknown_component_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let mut draft = Draft::builtin("jade").unwrap();
    let android = draft.package.files["android/appearance.toml"].clone();
    draft.package.files.insert(
        "windows/appearance.toml".into(),
        b"default_variant='light'\n[variants.light]\ntext='#112233'\n".to_vec(),
    );
    draft.export_new(&dir.path().join("edited.zip")).unwrap();
    let loaded = Package::open(&dir.path().join("edited.zip")).unwrap();
    assert_eq!(loaded.files["android/appearance.toml"], android);
    assert!(loaded.metadata.id.ends_with(".copy"));
}

#[test]
fn external_modification_blocks_export_and_failed_save_keeps_draft() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.zip");
    Draft::builtin("plain")
        .unwrap()
        .export_new(&source)
        .unwrap();
    let mut draft = Draft::open(&source).unwrap();
    draft.package.metadata.name = "changed".into();
    draft.dirty = true;
    std::fs::write(&source, b"external replacement").unwrap();
    assert!(draft.export_new(&dir.path().join("output.zip")).is_err());
    assert!(draft.dirty);
    assert_eq!(draft.package.metadata.name, "changed");
    assert!(!dir.path().join("output.zip").exists());
}

#[test]
fn platform_removal_and_private_resource_copy_preserve_shared_and_other_data() {
    let mut draft = Draft::builtin("jade").unwrap();
    draft
        .package
        .metadata
        .resources
        .insert("paper".into(), "shared/assets/paper.png".into());
    draft
        .package
        .files
        .insert("shared/assets/paper.png".into(), vec![1, 2, 3]);
    assert_eq!(
        draft.copy_shared_resource("paper", "android").unwrap(),
        "platform:paper"
    );
    draft.remove_platform("windows").unwrap();
    assert!(draft.package.files.contains_key("shared/assets/paper.png"));
    assert_eq!(
        draft.package.files["android/assets/paper.png"],
        vec![1, 2, 3]
    );
    assert!(draft.remove_platform("android").is_err());
    draft.add_platform("windows").unwrap();
    assert!(draft.package.platforms.contains_key("windows"));
    assert!(draft.package.encoded().is_ok());
}
