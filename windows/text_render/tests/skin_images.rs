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
