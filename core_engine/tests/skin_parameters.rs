use core_engine::{
    skin::{parameters::*, Extensions},
    Config,
};
use std::collections::BTreeSet;

fn supported() -> BTreeSet<String> {
    [
        "container.zip/1",
        "windows.base/1",
        "android.base/1",
        "appearance.parameters/1",
    ]
    .map(str::to_owned)
    .into()
}

#[test]
fn old_configuration_preserves_argb_and_point_size() {
    let config = Config::from_str("[appearance]\nfont_size=23\nprimary_color=0x80123456\nbackground_color=0xffabcdef\nhighlight_color=0xff654321").unwrap();
    assert_eq!(config.skins.windows.preset, "legacy");
    let p = legacy(&config.appearance).unwrap();
    assert_eq!(p.color("code"), 0x80123456);
    assert_eq!(p.color("background"), 0xffabcdef);
    assert_eq!(p.number("font_size"), 23.0);
    assert_eq!(parse_color("#12345680").unwrap(), 0x80123456);
    assert_eq!(argb_to_color(0x80123456), "#12345680");
    assert!(parse_color("#+12345").is_err());
}

#[test]
fn overlays_and_system_constraints_have_fixed_precedence() {
    let mut package = builtin_package("jade").unwrap();
    // Empty component variant isolates shared and platform defaults in this example.
    package.files.insert(
        "windows/appearance.toml".into(),
        b"default_variant='light'\n[variants.light]\n".to_vec(),
    );
    package
        .metadata
        .defaults
        .insert("text".into(), "#111111".into());
    package
        .platforms
        .get_mut("windows")
        .unwrap()
        .defaults
        .insert("text".into(), "#222222".into());
    let p = resolve(
        &package,
        "windows",
        "jade",
        false,
        &Extensions::new(),
        false,
        &supported(),
    )
    .unwrap();
    assert_eq!(p.parameters.color("text"), 0xff222222);
    let overrides: Extensions = [
        ("text".into(), "#333333".into()),
        ("font_size".into(), 28.into()),
    ]
    .into();
    let p = resolve(
        &package,
        "windows",
        "jade",
        false,
        &overrides,
        false,
        &supported(),
    )
    .unwrap();
    assert_eq!(p.parameters.color("text"), 0xff333333);
    let p = resolve(
        &package,
        "windows",
        "jade",
        false,
        &overrides,
        true,
        &supported(),
    )
    .unwrap();
    assert_eq!(p.parameters.color("text"), 0xff000000);
    assert_eq!(p.parameters.number("font_size"), 28.0);
    assert_eq!(p.parameters.number("shadow"), 0.0);
}

#[test]
fn explicit_single_variant_fallback_never_inverts_colors() {
    let mut package = builtin_package("jade").unwrap();
    package.files.insert(
        "android/appearance.toml".into(),
        b"default_variant='light'\n[variants.light]\nbackground='#F0F0F0'".to_vec(),
    );
    let result = resolve(
        &package,
        "android",
        "jade",
        true,
        &Extensions::new(),
        false,
        &supported(),
    )
    .unwrap();
    assert_eq!(result.variant, "light");
    assert_eq!(result.parameters.color("background"), 0xfff0f0f0);
    assert_eq!(result.warnings.len(), 1);
}

#[test]
fn inactive_variant_illegal_metrics_and_layout_actions_cannot_enter_appearance() {
    for fragment in [
        "font_size=0",
        "key_gap=-1",
        "text='#GGGGGG'",
        "font_size='14'",
        "action='commit'",
        "font_size=nan",
    ] {
        let mut package = builtin_package("plain").unwrap();
        let text =
            format!("default_variant='light'\n[variants.light]\n[variants.dark]\n{fragment}");
        package
            .files
            .insert("windows/appearance.toml".into(), text.into_bytes());
        assert!(
            resolve(
                &package,
                "windows",
                "plain",
                false,
                &Extensions::new(),
                false,
                &supported()
            )
            .is_err(),
            "{fragment}"
        );
    }
}

#[test]
fn both_platforms_resolve_same_color_semantics_and_preserve_preferences() {
    for name in ["jade", "plain", "contrast"] {
        let package = builtin_package(name).unwrap();
        for dark in [false, true] {
            let windows = resolve(
                &package,
                "windows",
                name,
                dark,
                &Extensions::new(),
                false,
                &supported(),
            )
            .unwrap();
            let android = resolve(
                &package,
                "android",
                name,
                dark,
                &Extensions::new(),
                false,
                &supported(),
            )
            .unwrap();
            assert_eq!(windows.parameters, android.parameters);
        }
    }
    let mut config = Config::default();
    config.skins.android.preset = "jade".into();
    config
        .skins
        .android
        .overrides
        .insert("keyboard_height".into(), 300.into());
    let loaded = Config::from_str(&config.to_string_toml().unwrap()).unwrap();
    assert_eq!(loaded.skins, config.skins);
    assert_eq!(loaded.skins.windows.preset, "legacy");
}
