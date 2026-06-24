//! 外观样式面板。

use crate::state::{self, AppState};
use eframe::egui::Ui;

/// 预设色板（ARGB）。
const PRESETS: [u32; 7] = [
    0xFF1E88E5,
    0xFFE53935,
    0xFF43A047,
    0xFFFB8C00,
    0xFF8E24AA,
    0xFF546E7A,
    0xFF000000,
];

/// ITU-R BT.601 亮度阈值：低于此值用白色文字，高于此值用黑色文字。
const LUMINANCE_THRESHOLD: u32 = 128000;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("外观样式");
    ui.separator();

    // 迷你候选框预览
    preview(ui, state);

    // 字体大小
    ui.horizontal(|ui| {
        ui.label("候选框字体大小:");
        let mut size = state.config.appearance.font_size;
        if ui
            .add(eframe::egui::Slider::new(&mut size, 8..=32))
            .changed()
        {
            state.config.appearance.font_size = size;
            state.mark_dirty();
        }
    });

    color_row(ui, "主色", &mut state.config.appearance.primary_color, &mut state.dirty, &mut state.status_msg);
    color_row(ui, "背景色", &mut state.config.appearance.background_color, &mut state.dirty, &mut state.status_msg);
    color_row(ui, "高亮色", &mut state.config.appearance.highlight_color, &mut state.dirty, &mut state.status_msg);
}

fn color_row(
    ui: &mut Ui,
    label: &str,
    value: &mut u32,
    dirty: &mut bool,
    status_msg: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        // 色块预览
        let color = eframe::egui::Color32::from_rgb(
            ((*value >> 16) & 0xFF) as u8,
            ((*value >> 8) & 0xFF) as u8,
            (*value & 0xFF) as u8,
        );
        let (rect, _) = ui.allocate_exact_size(
            eframe::egui::vec2(24.0, 24.0),
            eframe::egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 2.0, color);

        // 预设色块
        for &preset in &PRESETS {
            let preset_color = eframe::egui::Color32::from_rgb(
                ((preset >> 16) & 0xFF) as u8,
                ((preset >> 8) & 0xFF) as u8,
                (preset & 0xFF) as u8,
            );
            if ui
                .add(
                    eframe::egui::Button::new("   ")
                        .fill(preset_color)
                        .min_size(eframe::egui::vec2(20.0, 20.0)),
                )
                .clicked()
            {
                *value = preset;
                state::set_dirty(dirty, status_msg);
            }
        }

        // 自定义按钮 → Win32 ChooseColor
        if ui.button("自定义…").clicked() {
            if let Some(picked) = crate::color_picker::pick_color(*value) {
                *value = picked;
                state::set_dirty(dirty, status_msg);
            }
        }

        // ARGB 文本输入（非法输入标红 + tooltip）
        let mut text = format!("0x{:08X}", *value);
        let te = eframe::egui::TextEdit::singleline(&mut text)
            .desired_width(90.0);
        let resp = ui.add(te);
        if resp.lost_focus() {
            match parse_argb(&text) {
                Ok(v) if v != *value => {
                    *value = v;
                    state::set_dirty(dirty, status_msg);
                }
                Ok(_) => {}
                Err(()) => {
                    resp.on_hover_text("格式应为 0xAARRGGBB 或 #RRGGBB")
                        .request_focus();
                    *status_msg = Some(
                        "⚠️ 颜色格式非法，应为 0xAARRGGBB 或 #RRGGBB".into(),
                    );
                }
            }
        }
    });
}

/// 解析颜色字符串。支持三种格式：
/// - `0xAARRGGBB` / `AARRGGBB`（8 位十六进制，含 Alpha）
/// - `#RRGGBB` / `RRGGBB`（6 位十六进制，Alpha 默认 0xFF）
pub(crate) fn parse_argb(s: &str) -> Result<u32, ()> {
    let s = s.trim();
    // 去掉 0x / 0X / # 前缀
    let s = s.trim_start_matches("0x").trim_start_matches("0X").trim_start_matches('#');
    if s.len() == 8 {
        u32::from_str_radix(s, 16).map_err(|_| ())
    } else if s.len() == 6 {
        Ok(0xFF000000 | u32::from_str_radix(s, 16).map_err(|_| ())?)
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_argb;

    #[test]
    fn parse_hex_format() {
        assert_eq!(parse_argb("#1E88E5").unwrap(), 0xFF1E88E5);
        assert_eq!(parse_argb("0xFF1E88E5").unwrap(), 0xFF1E88E5);
        assert_eq!(parse_argb("FF1E88E5").unwrap(), 0xFF1E88E5);
        assert_eq!(parse_argb("#FF0000").unwrap(), 0xFFFF0000);
    }

    #[test]
    fn parse_invalid_rejected() {
        assert!(parse_argb("not-a-color").is_err());
        assert!(parse_argb("#GGG").is_err());
        assert!(parse_argb("0x123").is_err());
        assert!(parse_argb("").is_err());
    }
}

fn preview(ui: &mut Ui, state: &AppState) {
    ui.group(|ui| {
        ui.label("预览:");
        let bg = eframe::egui::Color32::from_rgb(
            ((state.config.appearance.background_color >> 16) & 0xFF) as u8,
            ((state.config.appearance.background_color >> 8) & 0xFF) as u8,
            (state.config.appearance.background_color & 0xFF) as u8,
        );
        let hl = eframe::egui::Color32::from_rgb(
            ((state.config.appearance.highlight_color >> 16) & 0xFF) as u8,
            ((state.config.appearance.highlight_color >> 8) & 0xFF) as u8,
            (state.config.appearance.highlight_color & 0xFF) as u8,
        );
        let bg_luma = (bg.r() as u32) * 299 + (bg.g() as u32) * 587 + (bg.b() as u32) * 114;
        let text_color = if bg_luma > LUMINANCE_THRESHOLD { eframe::egui::Color32::BLACK } else { eframe::egui::Color32::WHITE };
        let size = state.config.appearance.font_size as f32;
        let font_id = eframe::egui::FontId::proportional(size);
        // 根据实际字体度量计算"1"的宽度，确保高亮色完全覆盖
        let digit_w = ui.ctx().fonts(|f| f.glyph_width(&font_id, '1'));
        let pad = 6.0;
        let hl_w = digit_w + pad * 2.0;
        let hl_h = size + pad + 2.0;
        let (rect, _) = ui.allocate_exact_size(
            eframe::egui::vec2(200.0, size + 20.0),
            eframe::egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 4.0, bg);
        let hl_rect = eframe::egui::Rect::from_min_size(
            rect.min + eframe::egui::vec2(4.0, 4.0),
            eframe::egui::vec2(hl_w, hl_h),
        );
        ui.painter().rect_filled(hl_rect, 2.0, hl);
        ui.painter().text(
            rect.min + eframe::egui::vec2(4.0 + pad, 8.0),
            eframe::egui::Align2::LEFT_TOP,
            "1 你好 2 世界",
            font_id,
            text_color,
        );
    });
}