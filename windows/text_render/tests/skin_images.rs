use core_engine::skin::{
    images::{Image, Mode, Style},
    parameters::builtin_package,
};
use std::{collections::BTreeMap, sync::Arc};
use text_render::{skin_images::decode, TextSystem};
fn fixture() -> (
    core_engine::skin::Package,
    core_engine::skin::images::Images,
) {
    let mut package = builtin_package("jade").unwrap();
    package.files.insert(
        "shared/assets/panel.png".into(),
        include_bytes!("../../../core_engine/tests/fixtures/skin-panel.png").to_vec(),
    );
    let image = Image {
        path: "shared/assets/panel.png".into(),
        width: 16,
        height: 16,
        style: Style {
            resource: "shared:panel".into(),
            mode: Mode::NineSlice,
            slice: [4; 4],
            content: [6; 4],
            opacity: 1.0,
        },
    };
    (
        package,
        BTreeMap::from([
            ("background".into(), image.clone()),
            ("selected".into(), image),
        ]),
    )
}
#[test]
fn decoded_resources_are_shared_and_released_and_corrupt_png_is_rejected() {
    let (mut package, images) = fixture();
    let cache = decode(&package, &images).unwrap();
    assert!(Arc::ptr_eq(
        &cache["background"].pixels,
        &cache["selected"].pixels
    ));
    let weak = Arc::downgrade(&cache["background"].pixels);
    drop(cache);
    assert!(weak.upgrade().is_none());
    for _ in 0..1000 {
        let cache = decode(&package, &images).unwrap();
        assert_eq!(cache["background"].pixels.bgra.len(), 1024);
    }
    package.files.get_mut("shared/assets/panel.png").unwrap()[45] ^= 0xff;
    assert!(decode(&package, &images).is_err());
}
#[test]
fn compositing_preserves_premultiplication_across_dpi_and_narrow_sizes() {
    let (package, images) = fixture();
    let cache = decode(&package, &images).unwrap();
    let renderer = TextSystem::new().unwrap();
    for (w, h, scale) in [(6, 6, 1.0), (200, 60, 1.0), (300, 90, 1.5), (400, 120, 2.0)] {
        let text = renderer.render_alpha(w, h, 0, &[]).unwrap();
        let result = text.skin_background(&cache, scale, 0, &[]).unwrap();
        for pixel in result.pixels().as_chunks::<4>().0 {
            assert!(pixel[..3].iter().all(|v| *v <= pixel[3]));
        }
        assert!(result.pixels().iter().any(|v| *v != 0));
        let offset = (((h / 2) * w + w / 2) * 4) as usize;
        assert!(result.pixels()[offset + 3] > 0);
    }
}

#[test]
fn shipped_bamboo_template_uses_native_layout_and_rejects_bad_png() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/skins");
    let package = core_engine::skin::Package::open(&root.join("bamboo-dual.mywubi-skin")).unwrap();
    let supported = [
        "container.zip/1",
        "windows.base/1",
        "appearance.parameters/1",
        "appearance.images/1",
    ]
    .map(str::to_owned)
    .into();
    let resolved = core_engine::skin::parameters::resolve(
        &package,
        "windows",
        "jade",
        false,
        &Default::default(),
        false,
        &supported,
    )
    .unwrap();
    let images = Arc::new(decode(&package, &resolved.images).unwrap());
    let mut data = text_render::candidate_data::CandidateData {
        visible: true,
        spelling: "a".into(),
        ..Default::default()
    };
    data.theme =
        text_render::candidate_data::ThemeSnapshot::from_parameters(Arc::new(resolved.parameters));
    data.theme.images = images;
    data.items = ["工", "竹纸皮肤与长候选", "👩🏽‍💻"]
        .iter()
        .enumerate()
        .map(|(i, text)| text_render::candidate_data::CandidateItem {
            label: format!("{}. ", i + 1),
            text: (*text).into(),
            hint: "aaaa".into(),
            disabled: false,
            expression_kind: None,
        })
        .collect();
    let mut system = TextSystem::new().unwrap();
    let bitmap = text_render::preview::candidate(&mut system, &data, 600.0).unwrap();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/skin-acceptance/bamboo-windows-native.png");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let mut encoder = png::Encoder::new(
        std::fs::File::create(target).unwrap(),
        bitmap.width,
        bitmap.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut rgba = Vec::new();
    for [b, g, r, a] in bitmap.pixels().as_chunks::<4>().0 {
        for c in [r, g, b] {
            rgba.push(if *a == 0 {
                0
            } else {
                (u32::from(*c) * 255 / u32::from(*a)).min(255) as u8
            });
        }
        rgba.push(*a);
    }
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgba)
        .unwrap();
    let damaged =
        core_engine::skin::Package::open(&root.join("invalid-damaged-png.mywubi-skin")).unwrap();
    assert!(core_engine::skin::parameters::resolve(
        &damaged,
        "windows",
        "jade",
        false,
        &Default::default(),
        false,
        &supported
    )
    .is_err());
}
