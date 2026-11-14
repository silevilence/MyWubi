use core_engine::skin::{keyboard::*, parameters, Capabilities, Component, Package};
use std::collections::BTreeSet;

fn package() -> Package {
    let mut p = parameters::builtin_package("jade").unwrap();
    p.files.insert(
        "android/mappings/pairs.toml".into(),
        br#"
format=1
charset="ab"
modes=["Main"]
[groups]
pair="ab"
"#
        .to_vec(),
    );
    p.files.insert(
        "android/layouts/pairs.toml".into(),
        br#"
format=1
charset="ab"
modes=["Main"]
width=3.0
height=1.0
mapping="pairs"
fallback="builtin.26"
[[keys]]
id="a"
label="AB"
x=0.0
y=0.0
width=1.0
height=1.0
role="key"
action={kind="Group",value="pair"}
"#
        .to_vec(),
    );
    let platform = p.platforms.get_mut("android").unwrap();
    platform.components.push(Component {
        id: "pairs".into(),
        version: "1".into(),
        kind: "mapping".into(),
        path: "mappings/pairs.toml".into(),
        capabilities: Capabilities {
            required: vec![GROUP_CAPABILITY.into()],
            ..Default::default()
        },
        dependencies: Default::default(),
        extensions: Default::default(),
    });
    platform.components.push(Component {
        id: "layout".into(),
        version: "1".into(),
        kind: "layout".into(),
        path: "layouts/pairs.toml".into(),
        capabilities: Capabilities {
            required: vec![LAYOUT_CAPABILITY.into(), GROUP_CAPABILITY.into()],
            ..Default::default()
        },
        dependencies: [("pairs".into(), "1".into())].into(),
        extensions: Default::default(),
    });
    p
}
fn supported() -> BTreeSet<String> {
    [
        "container.zip/1",
        "android.base/1",
        "appearance.parameters/1",
        LAYOUT_CAPABILITY,
        GROUP_CAPABILITY,
    ]
    .map(str::to_owned)
    .into()
}
#[test]
fn groups_require_capability_and_exact_mapping_dependency() {
    let mut p = package();
    assert!(compatibility(&p, &supported()).unwrap()["layout"].enabled);
    let mut old = supported();
    old.remove(GROUP_CAPABILITY);
    let states = compatibility(&p, &old).unwrap();
    assert!(!states["layout"].enabled);
    assert!(p.platforms["android"]
        .components
        .iter()
        .filter(|c| c.kind == "appearance")
        .all(|c| states[&c.id].enabled));
    p.platforms
        .get_mut("android")
        .unwrap()
        .components
        .last_mut()
        .unwrap()
        .dependencies
        .insert("pairs".into(), "2".into());
    assert!(!compatibility(&p, &supported()).unwrap()["layout"].enabled);
}
#[test]
fn malformed_mapping_disables_dependents_but_preserves_package() {
    let mut p = package();
    let bytes = p.files.get_mut("android/mappings/pairs.toml").unwrap();
    *bytes = String::from_utf8(bytes.clone())
        .unwrap()
        .replace("pair=\"ab\"", "pair=\"aa\"")
        .into_bytes();
    let states = compatibility(&p, &supported()).unwrap();
    assert!(!states["pairs"].enabled);
    assert!(!states["layout"].enabled);
    assert!(p.platforms.contains_key("windows"));
}
#[test]
fn multiple_and_layout_only_components_use_builtin_appearance() {
    let mut p = package();
    let platform = p.platforms.get_mut("android").unwrap();
    let mut extra = platform.components.last().unwrap().clone();
    extra.id = "second".into();
    extra.path = "layouts/second.toml".into();
    platform.components.retain(|c| c.kind != "appearance");
    platform.components.push(extra);
    p.files.insert(
        "android/layouts/second.toml".into(),
        p.files["android/layouts/pairs.toml"].clone(),
    );
    let states = compatibility(&p, &supported()).unwrap();
    assert!(states["layout"].enabled && states["second"].enabled);
    let (layout, mapping) = layout(&p, &p.platforms["android"].components[1]).unwrap();
    let encoded = toml::to_string(&layout).unwrap();
    let decoded: Layout = toml::from_str(&encoded).unwrap();
    assert_eq!(decoded.keys[0].action, KeyAction::Group("pair".into()));
    assert_eq!(mapping.unwrap().groups["pair"], "ab");
}
