use text_render::{Draw, TextSystem};

#[test]
fn transparent_round_frame_has_premultiplied_edges_and_no_opaque_background_leak() {
    let system = TextSystem::new().unwrap();
    let content = system.render_alpha(100, 60, 0x80ffffff, &[]).unwrap();
    let center = &content.pixels()[(30 * 100 + 50) * 4..][..4];
    assert!((127..=129).contains(&center[3]));
    let frame = content.framed(12.0, 1.0, 0xff245d51, 4.0).unwrap();
    assert_eq!((frame.width, frame.height), (116, 76));
    assert_eq!(&frame.pixels()[..4], &[0, 0, 0, 0]);
    for pixel in frame.pixels().as_chunks::<4>().0.iter() {
        assert!(pixel[..3].iter().all(|c| *c <= pixel[3]));
    }
    assert!((127..=129).contains(&frame.pixels()[((38 * 116 + 58) * 4) + 3]));
}

#[test]
fn opaque_legacy_render_and_colored_text_remain_opaque() {
    let mut system = TextSystem::new().unwrap();
    let text = system.layout("你好 👩🏽‍💻 é", "Segoe UI", 20.0, 300.0).unwrap();
    let bitmap = system
        .render(
            320,
            80,
            0x00123456,
            &[Draw::Text {
                layout: &text,
                x: 8.0,
                y: 8.0,
                color: 0xffffffff,
            }],
        )
        .unwrap();
    assert!(bitmap.pixels().as_chunks::<4>().0.iter().all(|p| p[3] == 255));
}
