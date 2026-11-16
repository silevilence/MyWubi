//! Focusless special-input palette: pinned controls and a clipped, measured grid.
use crate::{
    candidate_data::*, candidate_layout::foreground, coverage::inspect, Bitmap, Draw, TextLayout,
    TextSystem,
};
use core_engine::expressions::ExpressionKind;
use windows::core::Result;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}
impl Rect {
    fn width(self) -> f32 {
        self.right - self.left
    }
    fn height(self) -> f32 {
        self.bottom - self.top
    }
    fn moved(self, dy: f32) -> Self {
        Self {
            top: self.top + dy,
            bottom: self.bottom + dy,
            ..self
        }
    }
}
pub struct Element {
    pub rect: Rect,
    pub id: Option<usize>,
    pub name: String,
    pub selected: bool,
    pub scrolling: bool,
    pub texts: Vec<(TextLayout, f32, f32, u32)>,
    fill: Option<u32>,
}
pub struct BrowserScene {
    pub elements: Vec<Element>,
    pub width: f32,
    pub height: f32,
    pub viewport: Rect,
    pub scroll_limit: f32,
    background: u32,
    border: u32,
    scale: f32,
}

fn blend(a: u32, b: u32, weight: u32) -> u32 {
    let channel = |shift: u32| -> u32 {
        ((((a >> shift) & 255) * (100 - weight) + ((b >> shift) & 255) * weight) / 100) << shift
    };
    0xff000000 | channel(16) | channel(8) | channel(0)
}

impl BrowserScene {
    pub fn build(
        system: &mut TextSystem,
        data: &CandidateData,
        size: f32,
        max_width: f32,
        max_height: f32,
    ) -> Result<Self> {
        let size = size.min(max_height / 12.0).max(10.0);
        let scale = size / 19.0;
        let width = max_width.min(500.0 * scale);
        let height = max_height.min(490.0 * scale);
        let pad = 12.0 * scale;
        let gap = 6.0 * scale;
        let background = data.theme.background_color;
        let p = data.theme.parameters.as_deref();
        let text = p.map_or_else(|| foreground(background), |p| p.color("text"));
        let hint = p.map_or(blend(background, text, 72), |p| p.color("hint"));
        let selected = data.theme.highlight_color;
        let selected_text = p.map_or_else(|| foreground(selected), |p| p.color("selected_text"));
        let key_fill = blend(background, text, 5);
        let disabled = blend(background, text, 42);
        let mut scene = Self {
            elements: Vec::new(),
            width,
            height,
            viewport: Rect {
                left: pad,
                top: 90.0 * scale,
                right: width - pad,
                bottom: height - 72.0 * scale,
            },
            scroll_limit: 0.0,
            background,
            border: blend(background, text, 20),
            scale,
        };
        scene.control(
            system,
            Rect {
                left: pad,
                top: pad,
                right: width - 164.0 * scale,
                bottom: 46.0 * scale,
            },
            "特殊输入 · 全部",
            None,
            text,
            None,
            false,
        )?;
        scene.control(
            system,
            Rect {
                left: width - 160.0 * scale,
                top: pad,
                right: width - 88.0 * scale,
                bottom: 46.0 * scale,
            },
            "搜索",
            Some(EDIT_EXPRESSION),
            text,
            Some(key_fill),
            false,
        )?;
        scene.control(
            system,
            Rect {
                left: width - 82.0 * scale,
                top: pad,
                right: width - pad,
                bottom: 46.0 * scale,
            },
            "关闭",
            Some(CANCEL_EXPRESSION),
            text,
            Some(key_fill),
            false,
        )?;
        let Some(browser) = &data.browser else {
            return Ok(scene);
        };
        let kind = data.expression.as_ref().map(|q| q.kind);
        let tab_width = (width - 2.0 * pad - 5.0 * gap) / 6.0;
        for (index, category) in browser.categories.iter().enumerate() {
            let active = Some(category.kind) == kind;
            let label = match category.kind {
                ExpressionKind::Math => "数学",
                ExpressionKind::Symbols => "其他符号",
                _ => category.kind.label(),
            };
            let x = pad + index as f32 * (tab_width + gap);
            scene.control(
                system,
                Rect {
                    left: x,
                    top: 52.0 * scale,
                    right: x + tab_width,
                    bottom: 82.0 * scale,
                },
                label,
                category.unavailable.is_none().then_some(category.id),
                if category.unavailable.is_some() {
                    disabled
                } else if active {
                    selected_text
                } else {
                    text
                },
                Some(if active { selected } else { key_fill }),
                active,
            )?;
            if let Some(reason) = &category.unavailable {
                if let Some(tab) = scene.elements.last_mut() {
                    tab.name = format!("{label}，{reason}");
                }
            }
        }
        let columns = if kind == Some(ExpressionKind::Kaomoji) {
            2
        } else {
            6
        };
        let cell_width = (scene.viewport.width() - (columns - 1) as f32 * gap) / columns as f32;
        let mut y = scene.viewport.top;
        for (row_index, row) in data.items.chunks(columns).enumerate() {
            let first = scene.elements.len();
            let mut row_height = 76.0 * scale;
            for (column, item) in row.iter().enumerate() {
                let index = row_index * columns + column;
                let family = if item.expression_kind == Some(ExpressionKind::Emoji) {
                    "Segoe UI Emoji"
                } else {
                    "Segoe UI"
                };
                let glyph_size = if columns == 2 { size } else { size * 1.45 };
                let raw = system.layout(
                    &item.text,
                    family,
                    glyph_size,
                    (cell_width - 12.0 * scale).max(1.0),
                )?;
                let unsupported = inspect(&raw)?
                    .needs_fallback(item.expression_kind == Some(ExpressionKind::Emoji));
                let label = if unsupported {
                    format!("{} · 字体不支持完整显示", item.hint)
                } else {
                    item.hint.clone()
                };
                let glyph = if unsupported {
                    system.layout("□", "Segoe UI", glyph_size, cell_width - 12.0 * scale)?
                } else {
                    raw
                };
                let name = system.single_line(
                    &label,
                    "Segoe UI",
                    size * 0.65,
                    cell_width - 12.0 * scale,
                )?;
                let cell_height = (glyph.height + name.height + 20.0 * scale).max(76.0 * scale);
                row_height = row_height.max(cell_height);
                let x = pad + column as f32 * (cell_width + gap);
                let active = index == data.highlighted;
                let color = if active { selected_text } else { text };
                scene.elements.push(Element {
                    rect: Rect {
                        left: x,
                        top: y,
                        right: x + cell_width,
                        bottom: y + cell_height,
                    },
                    id: data.selection_ids.get(index).copied(),
                    name: format!("{}，{}", item.text, label),
                    selected: active,
                    scrolling: true,
                    texts: vec![
                        (
                            glyph.clone(),
                            x + (cell_width - glyph.width) / 2.0,
                            y + 8.0 * scale,
                            color,
                        ),
                        (
                            name.clone(),
                            x + (cell_width - name.width) / 2.0,
                            y + glyph.height + 12.0 * scale,
                            if active { selected_text } else { hint },
                        ),
                    ],
                    fill: Some(if active { selected } else { key_fill }),
                });
            }
            for cell in &mut scene.elements[first..] {
                cell.rect.bottom = y + row_height;
            }
            y += row_height + gap;
        }
        scene.scroll_limit = (y - gap - scene.viewport.bottom).max(0.0);
        let footer = height - 64.0 * scale;
        let previous = data.controls.iter().any(|c| c.id == PREVIOUS_PAGE);
        let next = data.controls.iter().any(|c| c.id == NEXT_PAGE);
        scene.control(
            system,
            Rect {
                left: pad,
                top: footer,
                right: pad + 80.0 * scale,
                bottom: footer + 28.0 * scale,
            },
            "上一页",
            previous.then_some(PREVIOUS_PAGE),
            if previous { text } else { disabled },
            Some(key_fill),
            false,
        )?;
        scene.control(
            system,
            Rect {
                left: width - pad - 80.0 * scale,
                top: footer,
                right: width - pad,
                bottom: footer + 28.0 * scale,
            },
            "下一页",
            next.then_some(NEXT_PAGE),
            if next { text } else { disabled },
            Some(key_fill),
            false,
        )?;
        scene.control(
            system,
            Rect {
                left: pad + 86.0 * scale,
                top: footer,
                right: width - pad - 86.0 * scale,
                bottom: footer + 28.0 * scale,
            },
            &format!(
                "{}–{} / {}",
                browser.offset + 1,
                browser.offset + data.items.len(),
                browser.total
            ),
            None,
            hint,
            None,
            false,
        )?;
        scene.control(
            system,
            Rect {
                left: pad,
                top: height - 30.0 * scale,
                right: width - pad,
                bottom: height - 6.0 * scale,
            },
            "方向键选择 · 回车上屏 · PgUp/PgDn 翻页 · Tab 搜索",
            None,
            hint,
            None,
            false,
        )?;
        Ok(scene)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "native cell styling is explicit at each layout site"
    )]
    fn control(
        &mut self,
        system: &mut TextSystem,
        rect: Rect,
        label: &str,
        id: Option<usize>,
        color: u32,
        fill: Option<u32>,
        selected: bool,
    ) -> Result<()> {
        let layout = system.single_line(
            label,
            "Segoe UI",
            14.0 * self.scale,
            (rect.width() - 8.0 * self.scale).max(1.0),
        )?;
        let x = rect.left + (rect.width() - layout.width) / 2.0;
        let y = rect.top + (rect.height() - layout.height) / 2.0;
        self.elements.push(Element {
            rect,
            id,
            name: label.into(),
            selected,
            scrolling: false,
            texts: vec![(layout, x, y, color)],
            fill,
        });
        Ok(())
    }

    pub fn reveal(&self, id: usize, scroll: f32) -> f32 {
        let Some(cell) = self
            .elements
            .iter()
            .find(|e| e.scrolling && e.id == Some(id))
        else {
            return scroll;
        };
        if cell.rect.top < self.viewport.top + scroll {
            cell.rect.top - self.viewport.top
        } else if cell.rect.bottom > self.viewport.bottom + scroll {
            (cell.rect.bottom - self.viewport.bottom).min(cell.rect.top - self.viewport.top)
        } else {
            scroll
        }
    }

    pub fn hits(&self, scroll: f32) -> Vec<(Rect, usize, String, bool)> {
        self.elements
            .iter()
            .filter_map(|e| {
                let id = e.id?;
                let mut r = if e.scrolling {
                    e.rect.moved(-scroll)
                } else {
                    e.rect
                };
                if e.scrolling {
                    r.top = r.top.max(self.viewport.top);
                    r.bottom = r.bottom.min(self.viewport.bottom);
                }
                (r.bottom > r.top).then(|| (r, id, e.name.clone(), e.selected))
            })
            .collect()
    }

    pub fn render(&self, system: &mut TextSystem, scroll: f32) -> Result<Bitmap> {
        let mut draws = Vec::new();
        for scrolling in [true, false] {
            if scrolling {
                draws.push(Draw::PushClip {
                    left: self.viewport.left,
                    top: self.viewport.top,
                    right: self.viewport.right,
                    bottom: self.viewport.bottom,
                });
            }
            for cell in self.elements.iter().filter(|e| e.scrolling == scrolling) {
                let dy = if scrolling { -scroll } else { 0.0 };
                let r = cell.rect.moved(dy);
                if scrolling && (r.bottom <= self.viewport.top || r.top >= self.viewport.bottom) {
                    continue;
                }
                if let Some(color) = cell.fill {
                    draws.push(Draw::RoundFill {
                        left: r.left,
                        top: r.top,
                        right: r.right,
                        bottom: r.bottom,
                        radius: 5.0 * self.scale,
                        color: self.border,
                    });
                    draws.push(Draw::RoundFill {
                        left: r.left + 1.0,
                        top: r.top + 1.0,
                        right: r.right - 1.0,
                        bottom: r.bottom - 1.0,
                        radius: 5.0 * self.scale,
                        color,
                    });
                }
                for (layout, x, y, color) in &cell.texts {
                    draws.push(Draw::Text {
                        layout,
                        x: *x,
                        y: *y + dy,
                        color: *color,
                    });
                }
            }
            if scrolling {
                draws.push(Draw::PopClip);
            }
        }
        if self.scroll_limit > 0.0 {
            let track = self.viewport.height();
            let thumb = (track * track / (track + self.scroll_limit)).max(16.0 * self.scale);
            let top = self.viewport.top + (track - thumb) * scroll / self.scroll_limit;
            draws.push(Draw::RoundFill {
                left: self.width - 7.0 * self.scale,
                top,
                right: self.width - 4.0 * self.scale,
                bottom: top + thumb,
                radius: 1.5 * self.scale,
                color: self.border,
            });
        }
        system
            .render(
                self.width.ceil() as u32,
                self.height.ceil() as u32,
                self.background,
                &draws,
            )?
            .framed(8.0 * self.scale, 1.0, self.border, 0.0)
    }
}
