//! `cargo run -p text_render --example coverage --release --offline -- target/em06-render --full`
//! Uses the same shaping and drawing path as the candidate window/settings preview.
use std::{collections::BTreeSet, error::Error, path::Path};
use text_render::{coverage::inspect, Bitmap, Draw, TextSystem};

fn bmp(path: &Path, bitmap: &Bitmap) -> std::io::Result<()> {
    let len = bitmap.pixels().len() as u32;
    let mut bytes = Vec::with_capacity(len as usize + 54);
    bytes.extend(b"BM");
    bytes.extend((len + 54).to_le_bytes());
    bytes.extend([0u8; 4]);
    bytes.extend(54u32.to_le_bytes());
    bytes.extend(40u32.to_le_bytes());
    bytes.extend(bitmap.width.to_le_bytes());
    bytes.extend((-(bitmap.height as i32)).to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(32u16.to_le_bytes());
    bytes.extend([0u8; 4]);
    bytes.extend(len.to_le_bytes());
    bytes.extend([0u8; 16]);
    bytes.extend(bitmap.pixels());
    std::fs::write(path, bytes)
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args().nth(1).ok_or("output directory required")?;
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;
    let mut system = TextSystem::new()?;
    let samples = [
        ("禁止自行车 🚳   笑脸 😀   红心 ❤️   文本心 ♥︎", false),
        ("家庭 👨‍👩‍👧‍👦   肤色/性别 👩🏽‍⚕️   旗帜 🇨🇳   键帽 1️⃣", false),
        ("颜文字 (╯°□°）╯︵ ┻━┻    (๑•̀ㅂ•́)و✧    Ω∀∑∫∴∵", false),
        ("组合附加符 á ë̄   空格: [　][ ][ ]   新增 🫩 🫪", false),
    ];
    let mut lines = Vec::new();
    let mut report = Vec::new();
    for (text, emoji) in samples {
        let layout = system.layout(text, "Segoe UI", 28.0, 1050.0)?;
        let coverage = inspect(&layout)?;
        report.push(serde_json::json!({"text":text,"missing":coverage.missing_glyphs,"visible_glyphs":coverage.visible_glyphs,
            "families":coverage.families,"fallback":coverage.needs_fallback(emoji)}));
        lines.push(layout);
    }
    let mut y = 12.0;
    let draws: Vec<_> = lines
        .iter()
        .map(|layout| {
            let draw = Draw::Text {
                layout,
                x: 12.0,
                y,
                color: 0xff202020,
            };
            y += layout.height + 18.0;
            draw
        })
        .collect();
    let bitmap = system.render(1100, y.ceil() as u32, 0xffffffff, &draws)?;
    bmp(&out.join("samples.bmp"), &bitmap)?;
    let colored = bitmap
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0].abs_diff(p[1]) > 30 || p[1].abs_diff(p[2]) > 30)
        .count();
    std::fs::write(
        out.join("samples.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"colored_pixels":colored,"samples":report}))?,
    )?;
    println!("color pixels: {colored}");
    if colored == 0 {
        return Err("system color font path returned no colored pixels".into());
    }

    if std::env::args().any(|arg| arg == "--full") {
        use core_engine::expressions::{ExpressionKind, ExpressionLibrary};
        for (kind, name) in [
            (ExpressionKind::Emoji, "emoji"),
            (ExpressionKind::Kaomoji, "kaomoji"),
        ] {
            let library = ExpressionLibrary::load(&kind.default_path(), kind)?;
            let mut failures = Vec::new();
            let mut families = BTreeSet::new();
            let mut characters = BTreeSet::new();
            for entry in &library.document().entries {
                characters.extend(entry.text.chars());
                let layout = system.layout(
                    &entry.text,
                    if kind == ExpressionKind::Emoji {
                        "Segoe UI Emoji"
                    } else {
                        "Segoe UI"
                    },
                    24.0,
                    10000.0,
                )?;
                let coverage = inspect(&layout)?;
                families.extend(coverage.families.iter().cloned());
                if coverage.needs_fallback(kind == ExpressionKind::Emoji) {
                    failures.push(serde_json::json!({"name":entry.name,"text":entry.text,"missing":coverage.missing_glyphs,
                        "visible_glyphs":coverage.visible_glyphs,"families":coverage.families}));
                }
            }
            let mut missing_chars = Vec::new();
            for ch in &characters {
                if ch.is_control() || ch.is_whitespace() {
                    continue;
                }
                let layout = system.layout(&ch.to_string(), "Segoe UI", 24.0, 1000.0)?;
                if inspect(&layout)?.missing_glyphs > 0 {
                    missing_chars.push(format!("U+{:04X}", *ch as u32));
                }
            }
            println!(
                "{name}: {} entries, {} requiring fallback, {} distinct chars, {} missing chars",
                library.document().entries.len(),
                failures.len(),
                characters.len(),
                missing_chars.len()
            );
            std::fs::write(
                out.join(format!("{name}-coverage.json")),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "entries":library.document().entries.len(),"fallback_entries":failures.len(),"distinct_characters":characters.len(),
                    "missing_characters":missing_chars,"families":families,"failures":failures
                }))?,
            )?;
        }
    }
    Ok(())
}
