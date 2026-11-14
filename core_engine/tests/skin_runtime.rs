use core_engine::{
    skin::{
        parameters::builtin_package,
        preferences::Selection,
        runtime::{self, SkinRuntime},
    },
    Config,
};
use std::{io::Cursor, sync::Arc};

#[test]
fn failed_refresh_retains_whole_snapshot_and_old_resources() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let mut config = Config::default();
    config.skins.windows.preset = "jade".into();
    let skin = SkinRuntime::new(runtime::load(&path, &config, "windows", false, false).unwrap());
    let first = skin.snapshot();
    config.skins.windows.selection = Some(Selection {
        id: "missing".into(),
        version: "1".into(),
        component: "jade".into(),
    });
    assert!(skin
        .refresh(&path, &config, "windows", false, false)
        .is_err());
    assert!(Arc::ptr_eq(&first, &skin.snapshot()));
    let package = builtin_package("jade").unwrap();
    core_engine::skin::archive::import(
        &runtime::store_path(&path),
        Cursor::new(package.encoded().unwrap()),
    )
    .unwrap();
    config.skins.windows.selection.as_mut().unwrap().id = "mywubi.jade".into();
    skin.refresh(&path, &config, "windows", true, false)
        .unwrap();
    let current = skin.snapshot();
    assert!(current.package.is_some());
    assert_eq!(current.parameters.color("background"), 0xff191f1d);
    assert_eq!(first.parameters.color("background"), 0xfff4f7f5);
}

#[test]
fn switching_preserves_user_size_and_does_not_change_input_configuration() {
    let path = std::path::Path::new("config.toml");
    let original = Config::default();
    let mut config = original.clone();
    config.appearance.font_size = 25;
    config.skins.windows.preset = "plain".into();
    assert!(runtime::same_input_configuration(&original, &config));
    let skin = runtime::load(path, &config, "windows", true, false).unwrap();
    assert_eq!(skin.parameters.number("font_size"), 25.0);
    config.skins.windows.use_skin_metrics = true;
    assert_eq!(
        runtime::load(path, &config, "windows", true, false)
            .unwrap()
            .parameters
            .number("font_size"),
        14.0
    );
    config.basic.auto_commit_unique = !original.basic.auto_commit_unique;
    assert!(!runtime::same_input_configuration(&original, &config));
}

#[test]
fn repeated_switches_release_retired_snapshots() {
    let path = std::path::Path::new("config.toml");
    let mut config = Config::default();
    config.skins.windows.preset = "jade".into();
    let skin = SkinRuntime::new(runtime::load(path, &config, "windows", false, false).unwrap());
    let first = Arc::downgrade(&skin.snapshot());
    for index in 0..1000 {
        skin.refresh(path, &config, "windows", index % 2 == 0, false)
            .unwrap();
    }
    assert!(first.upgrade().is_none());
    assert_eq!(Arc::strong_count(&skin.snapshot()), 2);
}
