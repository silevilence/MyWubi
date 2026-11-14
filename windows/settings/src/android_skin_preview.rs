//! Contract-based schematic; never represented as Android device pixel evidence.
use core_engine::skin::parameters::Parameters;
use eframe::egui::{self, Color32};

pub struct AndroidPreview {
    width: u32,
    landscape: bool,
    font_scale: f32,
    symbols: bool,
    pressed: bool,
    disabled: bool,
    bubble: bool,
}
impl Default for AndroidPreview {
    fn default() -> Self {
        Self {
            width: 393,
            landscape: false,
            font_scale: 1.0,
            symbols: false,
            pressed: false,
            disabled: false,
            bubble: true,
        }
    }
}
impl AndroidPreview {
    pub fn ui(&mut self, ui: &mut egui::Ui, p: &Parameters) {
        ui.label("Android 示意预览 · Windows 字体与系统渲染可能不同，需在 Android 原生预览核对");
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_label("屏宽 dp")
                .selected_text(self.width.to_string())
                .show_ui(ui, |ui| {
                    for width in [320, 393, 720] {
                        ui.selectable_value(&mut self.width, width, width.to_string());
                    }
                });
            ui.checkbox(&mut self.landscape, "横屏");
            ui.checkbox(&mut self.symbols, "数字符号");
            ui.checkbox(&mut self.pressed, "按下态");
            ui.checkbox(&mut self.disabled, "禁用态");
            ui.checkbox(&mut self.bubble, "气泡");
            ui.add(egui::Slider::new(&mut self.font_scale, 1.0..=2.0).text("字体缩放"));
        });
        let requested = if self.landscape {
            self.width.max(720)
        } else {
            self.width
        } as f32;
        let width = requested.min(ui.available_width());
        let scale = width / requested;
        let gap = p.number("key_gap") * scale;
        let key_height = (if self.landscape { 48.0 } else { 54.0 }) * scale;
        let bar = (p.number("font_size") * self.font_scale + 28.0).max(48.0) * scale;
        let top = if self.bubble { 55.0 * scale } else { 0.0 };
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(width, top + bar + 4.0 * (key_height + gap) + gap),
            egui::Sense::hover(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, color(p, "background"));
        let baseline = rect.top() + top;
        painter.text(
            egui::pos2(rect.left() + 8.0 * scale, baseline + bar / 2.0),
            egui::Align2::LEFT_CENTER,
            "a   1. 工   2. 戈   3. 或",
            egui::FontId::proportional(p.number("font_size") * self.font_scale * scale),
            color(p, "text"),
        );
        let rows = if self.symbols {
            vec!["1234567890", "@#$%&*()-", "!?.,:;\"'"]
        } else {
            vec!["qwertyuiop", "asdfghjkl", "zxcvbnm"]
        };
        for (row, letters) in rows.into_iter().enumerate() {
            let keys: Vec<_> = letters.chars().map(|c| c.to_string()).collect();
            draw_row(
                painter,
                p,
                &keys,
                rect.left(),
                baseline + bar + row as f32 * (key_height + gap),
                width,
                key_height,
                gap,
                scale * self.font_scale,
                self.pressed,
                self.disabled,
                false,
            );
        }
        let keys = ["⇧", "123", "空格", "删除", "回车"].map(str::to_owned);
        draw_row(
            painter,
            p,
            &keys,
            rect.left(),
            baseline + bar + 3.0 * (key_height + gap),
            width,
            key_height,
            gap,
            scale * self.font_scale,
            self.pressed,
            self.disabled,
            true,
        );
        if self.bubble {
            let bubble = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, rect.top() + 25.0 * scale),
                egui::vec2(42.0 * scale, 46.0 * scale),
            );
            painter.rect_filled(
                bubble,
                p.number("key_radius") * scale,
                color(p, "bubble_background"),
            );
            painter.text(
                bubble.center(),
                egui::Align2::CENTER_CENTER,
                if self.symbols { "5" } else { "t" },
                egui::FontId::proportional(p.number("bubble_text_size") * self.font_scale * scale),
                color(p, "bubble_text"),
            );
        }
        ui.small("固定样例：普通候选、字母/符号键、功能键、按下/禁用状态及气泡。预览不派发输入。");
    }
}
#[allow(clippy::too_many_arguments)]
fn draw_row(
    painter: &egui::Painter,
    p: &Parameters,
    keys: &[String],
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    gap: f32,
    font_scale: f32,
    pressed: bool,
    disabled: bool,
    function: bool,
) {
    let key_width = (width - gap * (keys.len() + 1) as f32) / keys.len() as f32;
    for (index, key) in keys.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            egui::pos2(x + gap + (key_width + gap) * index as f32, y),
            egui::vec2(key_width, height),
        );
        let down = pressed && index == keys.len() / 2;
        let background = if down {
            "pressed_background"
        } else if function {
            "function_background"
        } else {
            "key_background"
        };
        let foreground = if disabled {
            "disabled"
        } else if down {
            "pressed_text"
        } else if function {
            "function_text"
        } else {
            "key_text"
        };
        painter.rect_filled(
            rect,
            p.number("key_radius").min(height / 2.0),
            color(p, background),
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            key,
            egui::FontId::proportional(p.number("key_text_size") * font_scale),
            color(p, foreground),
        );
    }
}
fn color(p: &Parameters, key: &str) -> Color32 {
    let [a, r, g, b] = p.color(key).to_be_bytes();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}
