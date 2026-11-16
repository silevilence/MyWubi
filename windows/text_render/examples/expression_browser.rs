//! Offline layout/render acceptance: cargo run -p text_render --example expression_browser -- target/expression-browser/render
use core_engine::{candidates::*, expressions::*};
use std::{error::Error, path::Path};
use text_render::{candidate_data::*, expression_browser::BrowserScene, TextSystem};

fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args().nth(1).ok_or("output directory required")?;
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;
    let mut system = TextSystem::new()?;
    let mut report = Vec::new();
    for kind in ExpressionKind::ALL {
        let library = ExpressionLibrary::load(&kind.default_path(), kind)?;
        let mut entries = library.document().entries.clone();
        if kind == ExpressionKind::Kaomoji {
            entries.sort_by_key(|e| std::cmp::Reverse(e.text.chars().count()));
        }
        for dark in [false, true] {
            for scale in [1.0, 1.5, 2.0] {
                let mut data = CandidateData {
                    visible: true,
                    expression: Some(ExpressionQueryView {
                        kind,
                        browsing: true,
                        input: ExpressionInput::Words,
                        phase: ExpressionPhase::Results,
                        text: String::new(),
                        words: vec![],
                        message: None,
                    }),
                    browser: Some(BrowserData {
                        choosing_category: false,
                        category_index: 0,
                        offset: 0,
                        total: entries.len(),
                        categories: ExpressionKind::ALL
                            .iter()
                            .enumerate()
                            .map(|(i, kind)| BrowserCategory {
                                kind: *kind,
                                id: BROWSER_CATEGORY_BASE + i,
                                unavailable: None,
                            })
                            .collect(),
                    }),
                    items: entries
                        .iter()
                        .take(BROWSER_PAGE_SIZE)
                        .map(|e| CandidateItem {
                            text: e.text.clone(),
                            hint: e.name.clone(),
                            label: String::new(),
                            expression_kind: Some(kind),
                            disabled: false,
                        })
                        .collect(),
                    selection_ids: (0..entries.len().min(BROWSER_PAGE_SIZE)).collect(),
                    controls: vec![CandidateControl {
                        id: NEXT_PAGE,
                        label: "下一页".into(),
                    }],
                    ..Default::default()
                };
                if dark {
                    data.theme.background_color = 0xff202420;
                    data.theme.highlight_color = 0xff395b49;
                }
                let scene = BrowserScene::build(&mut system, &data, 19.0 * scale, 1000.0, 900.0)?;
                assert!(scene.viewport.bottom > scene.viewport.top);
                let grid: Vec<_> = scene.elements.iter().filter(|e| e.scrolling).collect();
                assert_eq!(grid.len(), data.items.len());
                for (i, a) in grid.iter().enumerate() {
                    assert!(
                        a.rect.left >= scene.viewport.left
                            && a.rect.right <= scene.viewport.right + 0.1
                    );
                    for b in &grid[i + 1..] {
                        assert!(
                            a.rect.right <= b.rect.left
                                || b.rect.right <= a.rect.left
                                || a.rect.bottom <= b.rect.top
                                || b.rect.bottom <= a.rect.top,
                            "overlapping cells"
                        );
                    }
                }
                for scroll in [0.0, scene.scroll_limit] {
                    let hits = scene.hits(scroll);
                    assert!(hits
                        .iter()
                        .filter(|(_, id, _, _)| *id < BROWSER_PAGE_SIZE)
                        .all(|(r, _, _, _)| r.top >= scene.viewport.top
                            && r.bottom <= scene.viewport.bottom));
                    assert!(hits.iter().any(|(_, id, _, _)| *id == EDIT_EXPRESSION));
                    assert!(hits.iter().any(|(_, id, _, _)| *id == NEXT_PAGE));
                }
                let bitmap = scene.render(&mut system, 0.0)?;
                let file = format!(
                    "{}-{}-{}.png",
                    kind.slug(),
                    if dark { "dark" } else { "light" },
                    (scale * 100.0) as u32
                );
                let mut encoder = png::Encoder::new(
                    std::fs::File::create(out.join(&file))?,
                    bitmap.width,
                    bitmap.height,
                );
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut rgba = bitmap.pixels().to_vec();
                for pixel in rgba.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
                encoder.write_header()?.write_image_data(&rgba)?;
                report.push(serde_json::json!({"file":file,"width":bitmap.width,"height":bitmap.height,"items":grid.len(),"scroll_limit":scene.scroll_limit}));
                if kind == ExpressionKind::Emoji {
                    let browser = data.browser.as_mut().unwrap();
                    browser.choosing_category = true;
                    browser.category_index = ExpressionKind::ALL.len() - 1;
                    let picker =
                        BrowserScene::build(&mut system, &data, 19.0 * scale, 1000.0, 900.0)?;
                    let cells: Vec<_> = picker
                        .hits(0.0)
                        .into_iter()
                        .filter(|(_, id, _, _)| {
                            (BROWSER_CATEGORY_BASE
                                ..BROWSER_CATEGORY_BASE + ExpressionKind::ALL.len())
                                .contains(id)
                        })
                        .collect();
                    assert_eq!(cells.len(), ExpressionKind::ALL.len());
                    assert!(cells
                        .iter()
                        .all(|(r, _, _, _)| r.left >= picker.viewport.left
                            && r.right <= picker.viewport.right + 0.1
                            && r.top >= picker.viewport.top
                            && r.bottom <= picker.viewport.bottom + 0.1));
                    let bitmap = picker.render(&mut system, 0.0)?;
                    let file = format!(
                        "categories-{}-{}.png",
                        if dark { "dark" } else { "light" },
                        (scale * 100.0) as u32
                    );
                    let mut encoder = png::Encoder::new(
                        std::fs::File::create(out.join(&file))?,
                        bitmap.width,
                        bitmap.height,
                    );
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut rgba = bitmap.pixels().to_vec();
                    for pixel in rgba.as_chunks_mut::<4>().0 {
                        pixel.swap(0, 2);
                    }
                    encoder.write_header()?.write_image_data(&rgba)?;
                    report.push(serde_json::json!({"file":file,"categories":cells.len()}));
                }
            }
        }
    }
    std::fs::write(
        out.join("results.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{} palette layouts rendered and checked", report.len());
    Ok(())
}
