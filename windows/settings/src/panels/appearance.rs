//! 外观样式面板。

use crate::state::{self, AppState};
use eframe::egui::Ui;

/// 预设色板（ARGB）。
const PRESETS: [u32; 7] = [
    0xFF1E88E5, 0xFFE53935, 0xFF43A047, 0xFFFB8C00, 0xFF8E24AA, 0xFF546E7A, 0xFF000000,
];

pub fn show(ui: &mut Ui, state: &mut AppState) {
    crate::ui_style::heading(ui, "候选外观", "调整候选框的显示方式；保存后自动应用。");
    crate::ui_style::card(ui, "皮肤方案", |ui| {
        skin_preferences(ui, state);
        if let Some(selection) = &state.config.skins.windows.selection {
            ui.label(format!(
                "当前自定义皮肤：{} / {}",
                selection.id, selection.component
            ));
        }
        ui.add_space(4.0);
        if ui.button("打开皮肤编辑器…").clicked() {
            state.status_msg = Some(match crate::skin_app::launch(&state.config_path) {
                Ok(()) => "已打开皮肤编辑器。应用皮肤后，可重新加载这里的设置。".into(),
                Err(e) => format!("[ERR] {e}"),
            });
        }
        ui.small("制作、导入、导出和应用皮肤，在独立编辑器中完成。");
    });
    crate::ui_style::card(ui, "字号与颜色", |ui| {
        ui.add_enabled_ui(!state.config.skins.windows.use_skin_metrics, |ui| {
            ui.horizontal(|ui| {
                ui.label("候选字号");
                if ui
                    .add(
                        eframe::egui::Slider::new(&mut state.config.appearance.font_size, 8..=32)
                            .suffix(" pt"),
                    )
                    .changed()
                {
                    state.mark_dirty();
                }
            });
        });
        let legacy = state.config.skins.windows.selection.is_none()
            && state.config.skins.windows.preset == "legacy";
        if legacy {
            for (label, value) in [
                ("主色", &mut state.config.appearance.primary_color),
                ("背景色", &mut state.config.appearance.background_color),
                ("高亮色", &mut state.config.appearance.highlight_color),
            ] {
                color_row(
                    ui,
                    label,
                    value,
                    &mut state.dirty,
                    &mut state.status_msg,
                    &mut state.form_errors,
                );
            }
        } else {
            for label in ["主色", "背景色", "高亮色"] {
                state.form_errors.remove(label);
            }
            ui.small("颜色由所选皮肤管理，可在皮肤编辑器中修改。");
        }
    });
    crate::ui_style::card(ui, "实际候选预览", |ui| {
        crate::skin_app::active_preview(ui, &state.config, &state.config_path);
    });
}

fn skin_preferences(ui: &mut Ui, state: &mut AppState) {
    use core_engine::skin::preferences::Mode;
    let preference = &mut state.config.skins.windows;
    let before = preference.clone();
    eframe::egui::ComboBox::from_label("皮肤方案")
        .selected_text(if preference.selection.is_some() {
            "自定义皮肤"
        } else {
            match preference.preset.as_str() {
                "jade" => "墨绿",
                "plain" => "素简",
                "contrast" => "高对比",
                _ => "经典外观",
            }
        })
        .show_ui(ui, |ui| {
            for (id, label) in [
                ("legacy", "经典外观"),
                ("jade", "墨绿"),
                ("plain", "素简"),
                ("contrast", "高对比"),
            ] {
                if ui
                    .selectable_label(
                        preference.selection.is_none() && preference.preset == id,
                        label,
                    )
                    .clicked()
                {
                    preference.preset = id.into();
                    preference.selection = None;
                }
            }
        });
    eframe::egui::ComboBox::from_label("明暗模式")
        .selected_text(match preference.mode {
            Mode::System => "跟随系统",
            Mode::Light => "浅色",
            Mode::Dark => "深色",
        })
        .show_ui(ui, |ui| {
            for (mode, label) in [
                (Mode::System, "跟随系统"),
                (Mode::Light, "浅色"),
                (Mode::Dark, "深色"),
            ] {
                ui.selectable_value(&mut preference.mode, mode, label);
            }
        });
    ui.checkbox(&mut preference.use_skin_metrics, "采用皮肤推荐字号");
    ui.small("默认保留当前字号；经典外观可直接自定义颜色。");
    if *preference != before {
        state.mark_dirty();
    }
}

fn color_row(
    ui: &mut Ui,
    label: &str,
    value: &mut u32,
    dirty: &mut bool,
    status_msg: &mut Option<String>,
    errors: &mut std::collections::BTreeMap<String, String>,
) {
    ui.push_id(label, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(label);
            // 色块预览
            let color = eframe::egui::Color32::from_rgb(
                ((*value >> 16) & 0xFF) as u8,
                ((*value >> 8) & 0xFF) as u8,
                (*value & 0xFF) as u8,
            );
            let (rect, _) = ui
                .allocate_exact_size(eframe::egui::vec2(24.0, 24.0), eframe::egui::Sense::hover());
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

            let key = ui.make_persistent_id("argb-input");
            let (text, resp) =
                crate::ui_style::text_buffer(ui, "argb-input", &format!("#{:08X}", *value));
            if resp.changed() {
                state::set_dirty(dirty, status_msg);
            }
            let invalid = parse_argb(&text).is_err();
            if invalid {
                errors.insert(label.into(), format!("{label}：请输入有效的颜色值"));
            } else {
                errors.remove(label);
            }
            if invalid {
                ui.colored_label(crate::ui_style::ERROR, "请输入 #RRGGBB 或 AARRGGBB");
            }
            // Commit every valid edit, but keep the user's exact buffer (including six
            // digits on the way to eight). Navigation may stop rendering this page.
            if resp.changed() && !invalid {
                if let Ok(v) = parse_argb(&text) {
                    *value = v;
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(key, (format!("#{v:08X}"), text.clone()));
                    });
                }
            }
            if (resp.lost_focus()
                || (resp.has_focus() && ui.input(|i| i.key_pressed(eframe::egui::Key::Enter))))
                && !invalid
            {
                if let Ok(v) = parse_argb(&text) {
                    if v != *value {
                        *value = v;
                        state::set_dirty(dirty, status_msg);
                    }
                }
                ui.ctx().data_mut(|d| d.remove::<(String, String)>(key));
            }
            if !resp.has_focus() && !invalid {
                ui.ctx().data_mut(|d| d.remove::<(String, String)>(key));
            }
        });
    });
}

/// 解析颜色字符串。支持三种格式：
/// - `0xAARRGGBB` / `AARRGGBB`（8 位十六进制，含 Alpha）
/// - `#RRGGBB` / `RRGGBB`（6 位十六进制，Alpha 默认 0xFF）
pub(crate) fn parse_argb(s: &str) -> Result<u32, ()> {
    let s = s.trim();
    // 去掉 0x / 0X / # 前缀
    let s = s
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .trim_start_matches('#');
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
