use core_engine::skin::{valid_path, Package};
use std::collections::{BTreeMap, BTreeSet};

fn files() -> BTreeMap<String, Vec<u8>> {
    [
        (
            "metadata.toml",
            r#"format = 1
id = "sample"
version = "1"
name = "样例"
author = "MyWubi"
license = "MIT"
[capabilities]
required = ["container.zip/1"]
[platforms]
android = "android/metadata.toml"
windows = "windows/metadata.toml"
[resources]
shared_image = "shared/assets/a.bin"
[future]
keep = "根扩展"
"#,
        ),
        (
            "android/metadata.toml",
            r#"platform = "android"
[capabilities]
required = ["android.base/1"]
[[components]]
id = "paint"
version = "1"
kind = "appearance"
path = "paint.toml"
[components.capabilities]
required = ["appearance.parameters/1"]
[components.capabilities.optional]
"future.effect/2" = "plain"
[[components]]
id = "fourteen"
version = "1"
kind = "layout"
path = "layout.toml"
[components.capabilities]
required = ["input.groups/1"]
[[components]]
id = "dependent"
version = "1"
kind = "layout"
path = "dependent.toml"
[components.dependencies]
fourteen = "1"
[[components]]
id = "future"
version = "1"
kind = "future-kind"
path = "future.toml"
"#,
        ),
        (
            "windows/metadata.toml",
            "platform = 'windows'\n[capabilities]\nrequired = ['windows.base/999']",
        ),
        ("android/paint.toml", "variant = 'light'"),
        ("android/layout.toml", "keys = 14"),
        ("android/dependent.toml", "keys = 14"),
        (
            "android/future.toml",
            "future = { resource = 'shared:shared_image' }",
        ),
        ("shared/assets/a.bin", "opaque shared bytes"),
    ]
    .into_iter()
    .map(|(p, s)| (p.into(), s.as_bytes().to_vec()))
    .collect()
}

fn supported() -> BTreeSet<String> {
    [
        "container.zip/1",
        "android.base/1",
        "appearance.parameters/1",
    ]
    .map(str::to_owned)
    .into()
}

#[test]
fn unsupported_layout_does_not_disable_appearance_or_other_platform() {
    let p = Package::from_files(files()).unwrap();
    let (base, c) = p
        .compatibility(
            "android",
            &supported(),
            &["appearance".into(), "layout".into()].into(),
        )
        .unwrap();
    assert!(base.enabled);
    assert!(c["paint"].enabled);
    assert_eq!(c["paint"].fallbacks["future.effect/2"], "plain");
    assert!(!c["fourteen"].enabled);
    assert!(!c["dependent"].enabled);
    assert!(!c["future"].enabled);
    assert!(
        !p.compatibility("windows", &supported(), &BTreeSet::new())
            .unwrap()
            .0
            .enabled
    );
    assert!(p
        .compatibility("android", &BTreeSet::new(), &BTreeSet::new())
        .is_err());
}

#[test]
fn unknown_fields_and_resources_round_trip_without_interpretation() {
    let original = files();
    let p = Package::from_files(original.clone()).unwrap();
    let serialized = toml::to_string(&p.metadata).unwrap();
    assert!(serialized.contains("根扩展"));
    assert_eq!(p.files, original);
}

#[test]
fn paths_and_case_collisions_reject_entire_package() {
    for path in [
        "../x",
        "/x",
        "C:/x",
        "a\\b",
        "a/../b",
        "a//b",
        "a/NUL.txt",
        "a/LPT1",
        "a/b.",
        "a/b ",
        "a:stream",
    ] {
        assert!(!valid_path(path), "{path}");
    }
    for path in ["METADATA.toml", "android", "../escape"] {
        let mut f = files();
        f.insert(path.into(), b"bad".to_vec());
        assert!(Package::from_files(f).is_err(), "{path}");
    }
}

#[test]
fn missing_resources_and_mismatched_platform_are_structural_errors() {
    let mut f = files();
    f.remove("shared/assets/a.bin");
    assert!(Package::from_files(f).is_err());
    let mut f = files();
    f.insert(
        "windows/metadata.toml".into(),
        b"platform = 'android'".to_vec(),
    );
    assert!(Package::from_files(f).is_err());
}

#[test]
fn cycle_and_wrong_dependency_version_only_disable_their_components() {
    let mut p = Package::from_files(files()).unwrap();
    let platform = p.platforms.get_mut("android").unwrap();
    platform.components[1].capabilities.required.clear();
    platform.components[1]
        .dependencies
        .insert("dependent".into(), "1".into());
    let (_, states) = p
        .compatibility(
            "android",
            &supported(),
            &["appearance".into(), "layout".into()].into(),
        )
        .unwrap();
    assert!(states["paint"].enabled);
    assert!(!states["fourteen"].enabled);
    assert!(!states["dependent"].enabled);
    let platform = p.platforms.get_mut("android").unwrap();
    platform.components[1].dependencies.clear();
    platform.components[2]
        .dependencies
        .insert("fourteen".into(), "2".into());
    let (_, states) = p
        .compatibility(
            "android",
            &supported(),
            &["appearance".into(), "layout".into()].into(),
        )
        .unwrap();
    assert!(states["fourteen"].enabled);
    assert!(!states["dependent"].enabled);
}

#[test]
fn layout_only_package_is_valid_and_missing_platform_is_reported() {
    let mut f = files();
    f.insert("android/metadata.toml".into(), b"platform = 'android'\n[[components]]\nid='keys'\nversion='1'\nkind='layout'\npath='layout.toml'".to_vec());
    let p = Package::from_files(f).unwrap();
    assert!(p
        .compatibility("linux", &supported(), &BTreeSet::new())
        .is_err());
    assert_eq!(p.platforms["android"].components.len(), 1);
}
