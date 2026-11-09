#![cfg(windows)]
use text_render::{coverage::inspect, Draw, TextSystem};

#[test]
fn emoji_is_colored_and_opaque_and_missing_characters_are_reported() {
    let mut system = TextSystem::new().unwrap();
    let layout = system.layout("🚳", "Segoe UI Emoji", 40.0, 200.0).unwrap();
    assert!(!inspect(&layout).unwrap().needs_fallback(true));
    let bitmap = system
        .render(
            100,
            100,
            0xffffffff,
            &[Draw::Text {
                layout: &layout,
                x: 8.0,
                y: 8.0,
                color: 0xff000000,
            }],
        )
        .unwrap();
    assert!(bitmap
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[3] == 255));
    assert!(bitmap
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[0].abs_diff(p[2]) > 60));
    let missing = system
        .layout("\u{10ffff}", "Segoe UI", 20.0, 200.0)
        .unwrap();
    assert!(inspect(&missing).unwrap().needs_fallback(false));
}

#[test]
fn wrapped_kaomoji_keeps_combining_ink_inside_the_measured_bounds() {
    let mut system = TextSystem::new().unwrap();
    for size in [14.0, 28.0, 56.0] {
        let text = "(๑•̀ㅂ•́)و✧ (╯°□°）╯︵ ┻━┻ ā̈́".repeat(4);
        let layout = system.layout(&text, "Segoe UI", size, 220.0).unwrap();
        assert!(layout.height > size * 3.0);
        let width = layout.width.ceil() as u32 + 12;
        let height = layout.height.ceil() as u32 + 12;
        let bitmap = system
            .render(
                width,
                height,
                0xffffffff,
                &[Draw::Text {
                    layout: &layout,
                    x: 6.0,
                    y: 6.0,
                    color: 0xff000000,
                }],
            )
            .unwrap();
        for y in 0..height {
            for x in 0..width {
                if x < 3 || y < 3 || x >= width - 3 || y >= height - 3 {
                    let i = ((y * width + x) * 4) as usize;
                    assert_eq!(
                        &bitmap.pixels()[i..i + 3],
                        &[255; 3],
                        "ink outside measured bounds at {size}px: {x},{y}"
                    );
                }
            }
        }
    }
}
