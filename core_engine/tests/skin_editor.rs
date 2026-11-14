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
fn android_only_authoring_can_be_exported_without_a_device() {
    let dir = tempfile::tempdir().unwrap();
    let mut draft = Draft::builtin("jade").unwrap();
    draft.remove_platform("windows").unwrap();
    let path = "android/appearance.toml";
    let mut appearance: core_engine::skin::parameters::Appearance =
        toml::from_str(std::str::from_utf8(&draft.package.files[path]).unwrap()).unwrap();
    appearance
        .variants
        .get_mut("dark")
        .unwrap()
        .insert("pressed_background".into(), "#123456".into());
    draft.package.files.insert(
        path.into(),
        toml::to_string(&appearance).unwrap().into_bytes(),
    );
    let output = dir.path().join("android.zip");
    draft.export_new(&output).unwrap();
    let loaded = Package::open(&output).unwrap();
    let supported = [
        "container.zip/1".into(),
        "android.base/1".into(),
        "appearance.parameters/1".into(),
    ]
    .into();
    let effective = core_engine::skin::parameters::resolve(
        &loaded,
        "android",
        "jade",
        true,
        &Default::default(),
        false,
        &supported,
    )
    .unwrap();
    assert_eq!(effective.parameters.color("pressed_background"), 0xff123456);
    assert!(!loaded.platforms.contains_key("windows"));
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
