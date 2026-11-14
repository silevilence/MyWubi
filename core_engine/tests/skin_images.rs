use core_engine::skin::{
    images::{patches, Mode, Style, CAPABILITY},
    parameters::*,
    Extensions, Package,
};
use std::collections::BTreeSet;
fn package() -> Package {
    let mut p = builtin_package("jade").unwrap();
    p.metadata
        .resources
        .insert("panel".into(), "shared/assets/panel.png".into());
    p.files.insert(
        "shared/assets/panel.png".into(),
        include_bytes!("fixtures/skin-panel.png").to_vec(),
    );
    for platform in ["windows", "android"] {
        p.platforms.get_mut(platform).unwrap().components[0]
            .capabilities
            .required
            .push(CAPABILITY.into());
        let path = format!("{platform}/appearance.toml");
        let mut appearance: Appearance =
            toml::from_str(std::str::from_utf8(&p.files[&path]).unwrap()).unwrap();
        appearance.extensions.insert("images".into(),toml::from_str::<toml::Value>("[light.background]\nresource='shared:panel'\nmode='nine_slice'\nslice=[4,4,4,4]\ncontent=[6,6,6,6]\nopacity=0.8").unwrap());
        p.files
            .insert(path, toml::to_string(&appearance).unwrap().into_bytes());
    }
    p
}
fn supported() -> BTreeSet<String> {
    [
        "container.zip/1",
        "windows.base/1",
        "android.base/1",
        "appearance.parameters/1",
        CAPABILITY,
    ]
    .map(str::to_owned)
    .into()
}
#[test]
fn shared_image_has_independent_platform_styles_and_system_override() {
    let p = package();
    for platform in ["windows", "android"] {
        let r = resolve(
            &p,
            platform,
            "jade",
            false,
            &Extensions::new(),
            false,
            &supported(),
        )
        .unwrap();
        assert_eq!(r.images["background"].path, "shared/assets/panel.png");
        assert_eq!(r.images["background"].width, 16);
        assert!(resolve(
            &p,
            platform,
            "jade",
            false,
            &Extensions::new(),
            true,
            &supported()
        )
        .unwrap()
        .images
        .is_empty());
        assert!(resolve(
            &p,
            platform,
            "jade",
            true,
            &Extensions::new(),
            false,
            &supported()
        )
        .unwrap()
        .images
        .is_empty());
    }
}
#[test]
fn old_client_rejects_required_or_uses_explicit_plain_fallback() {
    let mut p = package();
    let mut old = supported();
    old.remove(CAPABILITY);
    assert!(resolve(
        &p,
        "windows",
        "jade",
        false,
        &Extensions::new(),
        false,
        &old
    )
    .is_err());
    let caps = &mut p.platforms.get_mut("windows").unwrap().components[0].capabilities;
    caps.required.retain(|v| v != CAPABILITY);
    caps.optional.insert(CAPABILITY.into(), "plain".into());
    let r = resolve(
        &p,
        "windows",
        "jade",
        false,
        &Extensions::new(),
        false,
        &old,
    )
    .unwrap();
    assert!(r.images.is_empty());
    assert!(!r.warnings.is_empty());
}
#[test]
fn missing_truncated_and_oversized_images_are_rejected() {
    let mut p = package();
    p.files
        .get_mut("shared/assets/panel.png")
        .unwrap()
        .truncate(35);
    assert!(resolve(
        &p,
        "windows",
        "jade",
        false,
        &Extensions::new(),
        false,
        &supported()
    )
    .is_err());
    p = package();
    p.files.get_mut("shared/assets/panel.png").unwrap()[16..20]
        .copy_from_slice(&2049u32.to_be_bytes());
    assert!(resolve(
        &p,
        "windows",
        "jade",
        false,
        &Extensions::new(),
        false,
        &supported()
    )
    .is_err());
    p = package();
    p.metadata.resources.clear();
    assert!(resolve(
        &p,
        "windows",
        "jade",
        false,
        &Extensions::new(),
        false,
        &supported()
    )
    .is_err());
}
#[test]
fn nine_slice_preserves_corners_and_never_overlaps_when_small() {
    let style = Style {
        resource: "shared:panel".into(),
        mode: Mode::NineSlice,
        slice: [4; 4],
        content: [6; 4],
        opacity: 1.0,
    };
    let normal = patches(&style, [16.0; 2], [100.0, 40.0], 2.0);
    assert_eq!(normal.len(), 9);
    assert_eq!(normal[0], ([0.0, 0.0, 4.0, 4.0], [0.0, 0.0, 8.0, 8.0]));
    let tiny = patches(&style, [16.0; 2], [6.0; 2], 2.0);
    assert_eq!(tiny.len(), 4);
    let area: f32 = tiny
        .iter()
        .map(|(_, r)| (r[2] - r[0]) * (r[3] - r[1]))
        .sum();
    assert_eq!(area, 36.0);
}
