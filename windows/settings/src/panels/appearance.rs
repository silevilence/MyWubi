//! 外观样式面板。

use crate::state::AppState;
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

    color_row(ui, &mut state.dirty, "主色", &mut state.config.appearance.primary_color);
    color_row(
        ui,
        &mut state.dirty,
        "背景色",
        &mut state.config.appearance.background_color,
    );
    color_row(
        ui,
        &mut state.dirty,
        "高亮色",
        &mut state.config.appearance.highlight_color,
    );
}

fn color_row(ui: &mut Ui, dirty: &mut bool, label: &str, value: &mut u32) {
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
                *dirty = true;
            }
        }

        // 自定义按钮 → Win32 ChooseColor
        if ui.button("自定义…").clicked() {
            if let Some(picked) = crate::color_picker::pick_color(*value) {
                *value = picked;
                *dirty = true;
            }
        }

        // ARGB 文本输入
        let mut text = format!("0x{:08X}", *value);
        if ui.text_edit_singleline(&mut text).lost_focus() {
            if let Ok(v) = parse_argb(&text) {
                if v != *value {
                    *value = v;
                    *dirty = true;
                }
            }
        }
    });
}

fn parse_argb(s: &str) -> Result<u32, ()> {
    let s = s.trim();
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    if s.len() == 8 {
        u32::from_str_radix(s, 16).map_err(|_| ())
    } else if s.len() == 6 {
        Ok(0xFF000000 | u32::from_str_radix(s, 16).map_err(|_| ())?)
    } else {
        Err(())
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
        let size = state.config.appearance.font_size as f32;
        let (rect, _) = ui.allocate_exact_size(
            eframe::egui::vec2(200.0, size + 16.0),
            eframe::egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 4.0, bg);
        // 高亮第一候选
        let hl_rect = eframe::egui::Rect::from_min_size(
            rect.min + eframe::egui::vec2(4.0, 4.0),
            eframe::egui::vec2(40.0, size + 8.0),
        );
        ui.painter().rect_filled(hl_rect, 2.0, hl);
        ui.painter().text(
            rect.min + eframe::egui::vec2(8.0, 8.0),
            eframe::egui::Align2::LEFT_TOP,
            "1 你好 2 世界",
            eframe::egui::FontId::proportional(size),
            eframe::egui::Color32::BLACK,
        );
    });
}