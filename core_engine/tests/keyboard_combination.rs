use core_engine::skin::{
    combination::*,
    keyboard::Mode,
    parameters,
    preferences::{Preference, Selection},
    Capabilities, Component, Package,
};
use std::{path::Path, sync::Arc};

fn fixture(version: &str, group: &str, role: &str) -> Package {
    let mut p = parameters::builtin_package("jade").unwrap();
    p.metadata.id = "keyboard.test".into();
    p.metadata.version = version.into();
    p.files.insert(
        "android/mappings/pairs.toml".into(),
        format!("format=1\ncharset='ab'\nmodes=['Main']\n[groups]\n{group}='ab'\n").into_bytes(),
    );
    p.files.insert(
        "android/layouts/pairs.toml".into(),
        format!(
            r#"format=1
charset="ab"
modes=["Main"]
width=2.0
height=1.0
mapping="pairs"
fallback="builtin.26"
[optional_styles]
custom="key"
[[keys]]
id="letters"
label="AB"
x=0.0
y=0.0
width=1.0
height=1.0
role="{role}"
action={{kind="Group",value="pair"}}
[[keys]]
id="delete"
label="Delete"
x=1.0
y=0.0
width=1.0
height=1.0
role="function"
action={{kind="Backspace"}}
"#
        )
        .into_bytes(),
    );
    let components = &mut p.platforms.get_mut("android").unwrap().components;
    components.push(Component {
        id: "pairs".into(),
        version: "1".into(),
        kind: "mapping".into(),
        path: "mappings/pairs.toml".into(),
        capabilities: Capabilities {
            required: vec!["input.groups/1".into()],
            ..Default::default()
        },
        dependencies: Default::default(),
        extensions: Default::default(),
    });
    components.push(Component {
        id: "layout".into(),
        version: "1".into(),
        kind: "layout".into(),
        path: "layouts/pairs.toml".into(),
        capabilities: Capabilities {
            required: vec!["keyboard.layout/1".into(), "input.groups/1".into()],
            ..Default::default()
        },
        dependencies: [("pairs".into(), "1".into())].into(),
        extensions: Default::default(),
    });
    p
}
fn choice(version: &str) -> Choice {
    Choice {
        layout: Selection {
            id: "keyboard.test".into(),
            version: version.into(),
            component: "layout".into(),
        },
        mapping: None,
        appearance: Preference::default(),
    }
}
fn install(path: &Path, p: &Package) {
    let dir = path.parent().unwrap().join("skins");
    std::fs::create_dir_all(&dir).unwrap();
    p.export_new(&dir.join(format!(
        "{}@{}.mywubi-skin",
        p.metadata.id, p.metadata.version
    )))
    .unwrap();
}

#[test]
fn failure_preserves_whole_snapshot_and_old_inflight_references() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    install(&path, &fixture("1", "pair", "key"));
    install(&path, &fixture("2", "deleted", "key"));
    install(&path, &fixture("3", "pair", "custom"));
    let runtime = KeyboardRuntime::new(resolve(&path, &choice("1"), Mode::Main, "ab").unwrap());
    let held = runtime.snapshot();
    assert!(runtime
        .refresh(&path, &choice("2"), Mode::Main, "ab")
        .is_err());
    assert!(Arc::ptr_eq(&held, &runtime.snapshot()));
    runtime
        .refresh(&path, &choice("3"), Mode::Main, "ab")
        .unwrap();
    assert_ne!(runtime.snapshot().revision, held.revision);
    assert_eq!(runtime.snapshot().role_fallbacks["custom"], "key");
    assert_eq!(held.mapping.as_ref().unwrap().groups["pair"], "ab");
    let weak = Arc::downgrade(&held);
    drop(held);
    assert!(weak.upgrade().is_none());
}
#[test]
fn interrupted_update_and_damaged_active_package_restore_previous_complete_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    install(&path, &fixture("1", "pair", "key"));
    install(&path, &fixture("2", "pair", "key"));
    let first = resolve(&path, &choice("1"), Mode::Main, "ab").unwrap();
    save(&path, &first).unwrap();
    std::fs::write(
        path.with_file_name("keyboard-selection.json.interrupted.tmp"),
        b"partial",
    )
    .unwrap();
    assert_eq!(
        restore(&path, Mode::Main, "ab").unwrap().digest,
        first.digest
    );
    let next = resolve(&path, &choice("2"), Mode::Main, "ab").unwrap();
    save(&path, &next).unwrap();
    std::fs::write(
        dir.path().join("skins/keyboard.test@2.mywubi-skin"),
        b"damaged",
    )
    .unwrap();
    assert_eq!(
        restore(&path, Mode::Main, "ab").unwrap().digest,
        first.digest
    );
}
#[test]
fn cross_package_missing_resources_fixed_artwork_and_character_mismatch_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut p = fixture("1", "pair", "key");
    let appearance = p.platforms["android"]
        .components
        .iter()
        .find(|c| c.kind == "appearance")
        .unwrap();
    let name = format!("android/{}", appearance.path);
    let bytes = p.files.get_mut(&name).unwrap();
    *bytes = [b"fixed_layout='builtin.26'\n".as_slice(), bytes.as_slice()].concat();
    install(&path, &p);
    let mut c = choice("1");
    c.mapping = Some(Selection {
        id: "missing".into(),
        version: "1".into(),
        component: "pairs".into(),
    });
    assert!(resolve(&path, &c, Mode::Main, "ab").is_err());
    c.mapping = None;
    assert!(resolve(&path, &c, Mode::Main, "abc").is_err());
    c.appearance.selection = Some(Selection {
        id: "keyboard.test".into(),
        version: "1".into(),
        component: "jade".into(),
    });
    assert!(resolve(&path, &c, Mode::Main, "ab")
        .err()
        .unwrap()
        .to_string()
        .contains("固定键位"));
}

#[test]
fn appearance_changes_keep_mapping_identity_and_repeated_save_keeps_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    install(&path, &fixture("1", "pair", "key"));
    install(&path, &fixture("2", "pair", "key"));
    let first = resolve(&path, &choice("1"), Mode::Main, "ab").unwrap();
    save(&path, &first).unwrap();
    let mut recolored = choice("1");
    recolored.appearance.preset = "contrast".into();
    let recolored = resolve(&path, &recolored, Mode::Main, "ab").unwrap();
    assert_eq!(first.mapping_identity, recolored.mapping_identity);
    assert_ne!(first.revision, recolored.revision);
    let second = resolve(&path, &choice("2"), Mode::Main, "ab").unwrap();
    save(&path, &second).unwrap();
    save(&path, &second).unwrap();
    std::fs::write(
        dir.path().join("skins/keyboard.test@2.mywubi-skin"),
        b"damaged",
    )
    .unwrap();
    assert_eq!(
        restore(&path, Mode::Main, "ab").unwrap().digest,
        first.digest
    );
}

#[test]
fn mode_selections_survive_restart_independently() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let package = core_engine::skin::keyboard_templates::package().unwrap();
    install(&path, &package);
    let choice = |component: &str| Choice {
        layout: Selection {
            id: package.metadata.id.clone(),
            version: package.metadata.version.clone(),
            component: component.into(),
        },
        mapping: None,
        appearance: Preference::default(),
    };
    let main = resolve(
        &path,
        &choice("shape14"),
        Mode::Main,
        "abcdefghijklmnopqrstuvwxyz",
    )
    .unwrap();
    let pinyin = resolve(
        &path,
        &choice("pinyin9"),
        Mode::Pinyin,
        "abcdefghijklmnopqrstuvwxyz'",
    )
    .unwrap();
    save(&path, &main).unwrap();
    save(&path, &pinyin).unwrap();
    assert_eq!(
        restore(&path, Mode::Main, "abcdefghijklmnopqrstuvwxyz")
            .unwrap()
            .digest,
        main.digest
    );
    assert_eq!(
        restore(&path, Mode::Pinyin, "abcdefghijklmnopqrstuvwxyz'")
            .unwrap()
            .digest,
        pinyin.digest
    );
    execute(
        serde_json::from_value(serde_json::json!({"action":"Reset","path":path,"mode":"Pinyin"}))
            .unwrap(),
    )
    .unwrap();
    assert!(restore(&path, Mode::Pinyin, "abcdefghijklmnopqrstuvwxyz'").is_err());
    assert_eq!(
        restore(&path, Mode::Main, "abcdefghijklmnopqrstuvwxyz")
            .unwrap()
            .digest,
        main.digest
    );
}
