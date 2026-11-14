//! Measured candidate layout; the same rectangles drive drawing and hit testing.
use crate::candidate_data::CandidateData;
use crate::{coverage::inspect, TextLayout, TextSpan, TextSystem};
use core_engine::{candidates::ExpressionPhase, expressions::ExpressionKind};
use windows::core::Result;

pub struct Block {
    pub layout: TextLayout,
    pub x: f32,
    pub y: f32,
    pub color: u32,
    pub background: Option<u32>,
    pub id: Option<usize>,
}

pub struct Scene {
    pub blocks: Vec<Block>,
    pub width: f32,
    pub height: f32,
    pub vertical: bool,
}

pub fn foreground(background: u32) -> u32 {
    let [_, r, g, b] = background.to_be_bytes();
    if u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 < 128_000 {
        0xfffafafa
    } else {
        0xff202020
    }
}

impl Scene {
    pub fn build(
        system: &mut TextSystem,
        data: &CandidateData,
        size: f32,
        max_width: f32,
    ) -> Result<Self> {
        let parameters = data.theme.parameters.as_deref();
        let dip = size / (f32::from(data.theme.font_size) * 96.0 / 72.0).max(1.0);
        let density = parameters.map_or(0.0, |p| {
            if p.text("density") == "comfortable" {
                4.0 * dip
            } else {
                0.0
            }
        });
        let image_insets = |role: &str| {
            data.theme
                .images
                .get(role)
                .map_or([0.0; 4], |i| i.image.style.content.map(|v| v as f32 * dip))
        };
        let background_insets = image_insets("background");
        let selected_insets = image_insets("selected");
        let pad = parameters
            .map_or((size * 0.4).ceil().max(6.0), |p| {
                p.number("padding_x") * dip + density
            })
            .max(2.0)
            .max(background_insets[0])
            .max(background_insets[2])
            .max(selected_insets[0])
            .max(selected_insets[2]);
        let gap = parameters
            .map_or(pad, |p| p.number("padding_y") * dip + density)
            .max(2.0)
            .max(background_insets[1])
            .max(background_insets[3])
            .max(selected_insets[1] + selected_insets[3]);
        let font = parameters.map_or("Segoe UI", |p| p.text("font_family"));
        let content_width = (max_width - pad * 2.0 - 6.0).max(size * 2.0);
        let color = parameters.map_or_else(
            || foreground(data.theme.background_color),
            |p| p.color("text"),
        );
        let mut scene = Self {
            blocks: Vec::new(),
            width: 200.0_f32.min(max_width),
            height: gap,
            vertical: false,
        };
        let mut header = Vec::new();
        if let Some(query) = &data.expression {
            header.push(format!(
                "{} · {}",
                data.mode_label,
                if query.phase == ExpressionPhase::Editing {
                    if query.input == core_engine::candidates::ExpressionInput::Literal {
                        "字面别名"
                    } else {
                        "主码表组词"
                    }
                } else {
                    "选择结果"
                }
            ));
            header.push(format!(
                "关键词：{}",
                if query.text.is_empty() {
                    "（尚未选词）"
                } else {
                    &query.text
                }
            ));
            if !data.spelling.is_empty() {
                header.push(format!("编码：{}", data.spelling));
            }
            if let Some(message) = &query.message {
                header.push(message.clone());
            }
        } else if !data.spelling.is_empty() {
            header.push(data.spelling.clone());
        } else if !data.mode_label.is_empty() {
            header.push(data.mode_label.clone());
        }
        for line in header {
            let layout = system.layout(&line, font, size, content_width)?;
            scene.push(
                Block {
                    layout,
                    x: pad,
                    y: scene.height,
                    color: data.theme.primary_color,
                    background: None,
                    id: None,
                },
                pad,
                gap,
            );
        }
        let mut items = Vec::new();
        for (index, item) in data.items.iter().enumerate() {
            let family = if item.expression_kind == Some(ExpressionKind::Emoji) {
                "Segoe UI Emoji"
            } else {
                font
            };
            let fallback = if item.expression_kind.is_some() {
                let raw = system.layout(&item.text, family, size, 100_000.0)?;
                inspect(&raw)?.needs_fallback(item.expression_kind == Some(ExpressionKind::Emoji))
            } else {
                false
            };
            let text = if fallback {
                let codes = item
                    .text
                    .chars()
                    .map(|c| format!("U+{:04X}", c as u32))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{}[{} · 字体不支持：{}]", item.label, item.hint, codes)
            } else {
                format!(
                    "{}{}{}{}",
                    item.label,
                    item.text,
                    if item.hint.is_empty() { "" } else { "  " },
                    item.hint
                )
            };
            let highlighted = index == data.highlighted && !item.disabled;
            let mut spans = Vec::new();
            if let Some(p) = parameters {
                let selected = |normal: &str, selected: &str| {
                    p.color(if item.disabled {
                        "disabled"
                    } else if highlighted {
                        selected
                    } else {
                        normal
                    })
                };
                spans.push(TextSpan {
                    start: 0,
                    length: text.encode_utf16().count() as u32,
                    color: selected("text", "selected_text"),
                    size,
                });
                if !fallback {
                    let label_len = item.label.encode_utf16().count() as u32;
                    spans.push(TextSpan {
                        start: 0,
                        length: label_len,
                        color: selected("label", "selected_text"),
                        size,
                    });
                    if !item.hint.is_empty() {
                        let start = label_len + item.text.encode_utf16().count() as u32 + 2;
                        spans.push(TextSpan {
                            start,
                            length: item.hint.encode_utf16().count() as u32,
                            color: selected("hint", "selected_hint"),
                            size: p.number("hint_size") * dip * 96.0 / 72.0,
                        });
                    }
                }
            }
            let layout = system.layout_styled(
                &text,
                if fallback { "Segoe UI" } else { family },
                size,
                content_width,
                &spans,
            )?;
            items.push(Block {
                layout,
                x: 0.0,
                y: 0.0,
                color: if highlighted {
                    foreground(data.theme.highlight_color)
                } else if item.disabled {
                    if color == 0xfffafafa {
                        0xffaaaaaa
                    } else {
                        0xff666666
                    }
                } else {
                    color
                },
                background: highlighted.then_some(data.theme.highlight_color),
                id: if item.disabled {
                    None
                } else {
                    data.selection_ids.get(index).copied()
                },
            });
        }
        let total = items
            .iter()
            .map(|item| item.layout.width + pad)
            .sum::<f32>();
        let layout = parameters.map_or("auto", |p| p.text("layout"));
        scene.vertical = layout == "vertical" || (layout == "auto" && total > content_width);
        if scene.vertical {
            for mut block in items {
                block.x = pad;
                block.y = scene.height;
                scene.push(block, pad, gap);
            }
        } else {
            let mut x = pad;
            let mut height = 0.0_f32;
            for mut block in items {
                block.x = x;
                block.y = scene.height;
                if x > pad && x + block.layout.width > max_width - pad {
                    scene.height += height + gap;
                    x = pad;
                    height = 0.0;
                    block.x = x;
                    block.y = scene.height;
                }
                x += block.layout.width + pad;
                height = height.max(block.layout.height);
                scene.blocks.push(block);
            }
            scene.width = scene.width.max(x + pad);
            scene.height += height + gap;
        }
        for notice in &data.notices {
            let layout = system.layout(notice, font, size, content_width)?;
            scene.push(
                Block {
                    layout,
                    x: pad,
                    y: scene.height,
                    color: parameters.map_or(color, |p| p.color("status")),
                    background: None,
                    id: None,
                },
                pad,
                gap,
            );
        }
        if data.total_pages > 1 {
            let layout = system.layout(
                &format!("第 {} / {} 页", data.page + 1, data.total_pages),
                "Segoe UI",
                size,
                content_width,
            )?;
            scene.push(
                Block {
                    layout,
                    x: pad,
                    y: scene.height,
                    color: parameters.map_or(color, |p| p.color("status")),
                    background: None,
                    id: None,
                },
                pad,
                gap,
            );
        }
        let mut x = pad;
        let mut row_height = 0.0_f32;
        for control in &data.controls {
            let layout = system.layout(
                &format!("[ {} ]", control.label),
                "Segoe UI",
                size,
                content_width,
            )?;
            if x > pad && x + layout.width + pad > max_width {
                scene.height += row_height + gap;
                x = pad;
                row_height = 0.0;
            }
            let width = layout.width;
            row_height = row_height.max(layout.height);
            scene.blocks.push(Block {
                layout,
                x,
                y: scene.height,
                color,
                background: None,
                id: Some(control.id),
            });
            x += width + pad;
            scene.width = scene.width.max(x + pad);
        }
        scene.height += row_height + gap;
        scene.width = scene.width.min(max_width).ceil();
        scene.height = scene.height.ceil();
        Ok(scene)
    }

    fn push(&mut self, block: Block, horizontal_gap: f32, vertical_gap: f32) {
        self.height = block.y + block.layout.height + vertical_gap;
        self.width = self
            .width
            .max(block.x + block.layout.width + horizontal_gap);
        self.blocks.push(block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_data::CandidateItem;

    #[test]
    fn semantic_styles_and_explicit_layouts_keep_whole_candidate_geometry() {
        use core_engine::skin::parameters;
        use std::sync::Arc;
        let mut system = TextSystem::new().unwrap();
        let mut data = CandidateData {
            visible: true,
            selection_ids: vec![7, 9, 11],
            ..Default::default()
        };
        data.items = (0..3)
            .map(|_| CandidateItem {
                expression_kind: None,
                disabled: false,
                label: "1. ".into(),
                text: "跨平台输入法".repeat(3),
                hint: "abcdef 反查".into(),
            })
            .collect();
        for mode in ["auto", "horizontal", "vertical"] {
            let mut p = parameters::builtin("jade", false).unwrap();
            p.0.insert("layout".into(), mode.into());
            p.0.insert("hint_size".into(), 28.into());
            data.theme = crate::candidate_data::ThemeSnapshot::from_parameters(Arc::new(p));
            for size in [18.0, 28.0, 56.0] {
                let scene = Scene::build(&mut system, &data, size, 400.0).unwrap();
                assert_eq!(
                    scene.blocks.iter().filter_map(|b| b.id).collect::<Vec<_>>(),
                    vec![7, 9, 11]
                );
                assert!(scene
                    .blocks
                    .iter()
                    .all(|b| b.x + b.layout.width <= scene.width));
                assert!(scene
                    .blocks
                    .iter()
                    .all(|b| b.y + b.layout.height <= scene.height));
                if mode == "horizontal" {
                    assert!(!scene.vertical);
                }
                if mode == "vertical" {
                    assert!(scene.vertical);
                }
            }
        }
    }

    #[test]
    fn long_expression_candidates_wrap_without_overlapping_or_losing_selection_ids() {
        let mut system = TextSystem::new().unwrap();
        let mut data = CandidateData {
            visible: true,
            selection_ids: (0..9).collect(),
            ..Default::default()
        };
        data.items = (0..9)
            .map(|i| CandidateItem {
                expression_kind: Some(ExpressionKind::Kaomoji),
                disabled: false,
                label: format!("{}. ", i + 1),
                text: "(๑•̀ㅂ•́)و✧ ".repeat(i + 2),
                hint: "开心 · 关键词匹配".into(),
            })
            .collect();
        for size in [14.0, 28.0, 56.0] {
            let scene = Scene::build(&mut system, &data, size, 350.0).unwrap();
            assert!(scene.vertical);
            assert!(scene.width <= 350.0);
            assert_eq!(
                scene.blocks.iter().filter_map(|b| b.id).collect::<Vec<_>>(),
                data.selection_ids
            );
            for blocks in scene.blocks.windows(2) {
                assert!(blocks[0].y + blocks[0].layout.height <= blocks[1].y);
            }
            assert!(scene
                .blocks
                .iter()
                .all(|b| b.x + b.layout.width <= scene.width));
        }
        data.items.truncate(2);
        for item in &mut data.items {
            item.text = "😀".into();
            item.hint.clear();
            item.expression_kind = Some(ExpressionKind::Emoji);
        }
        assert!(
            !Scene::build(&mut system, &data, 20.0, 800.0)
                .unwrap()
                .vertical
        );
    }
}
