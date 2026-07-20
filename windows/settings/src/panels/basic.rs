//! 常规设置面板。

use crate::state::AppState;
use core_engine::config::{CommitMode, PunctuationMode, SwitchKey};
use eframe::egui::Ui;

const HOTKEY_OPTIONS: [(&str, &str); 11] = [
    ("comma", "逗号 (, )"),
    ("period", "句号 (.)"),
    ("semicolon", "分号 (;)"),
    ("quote", "单引号 (')"),
    ("minus", "减号 (-)"),
    ("equal", "等号 (=)"),
    ("space", "空格"),
    ("left", "左箭头"),
    ("right", "右箭头"),
    ("page_down", "PageDown"),
    ("page_up", "PageUp"),
];

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("常规设置");
    ui.separator();

    // 候选词个数
    ui.horizontal(|ui| {
        ui.label("候选词个数:");
        let mut count = state.config.basic.candidate_count;
        if ui
            .add(eframe::egui::Slider::new(&mut count, 1..=10))
            .changed()
        {
            state.config.basic.candidate_count = count;
            state.mark_dirty();
        }
    });

    // 上屏方式
    ui.horizontal(|ui| {
        ui.label("上屏方式:");
        let mut mode = state.config.basic.commit_mode;
        let resp = eframe::egui::ComboBox::from_id_source("commit_mode")
            .selected_text(commit_mode_label(mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, CommitMode::SpaceFirst, "空格首选上屏");
                ui.selectable_value(&mut mode, CommitMode::EnterCommit, "回车上屏");
            });
        if resp.response.changed() && mode != state.config.basic.commit_mode {
            state.config.basic.commit_mode = mode;
            state.mark_dirty();
        }
    });

    // 中英文切换键
    ui.horizontal(|ui| {
        ui.label("中英文切换键:");
        let mut key = state.config.basic.switch_key;
        let resp = eframe::egui::ComboBox::from_id_source("switch_key")
            .selected_text(switch_key_label(key))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut key, SwitchKey::Shift, "Shift");
                ui.selectable_value(&mut key, SwitchKey::CapsLock, "CapsLock");
                ui.selectable_value(&mut key, SwitchKey::CtrlSpace, "Ctrl+Space");
            });
        if resp.response.changed() && key != state.config.basic.switch_key {
            state.config.basic.switch_key = key;
            state.mark_dirty();
        }
    });

    // 四码唯一自动上屏
    ui.horizontal(|ui| {
        let mut auto = state.config.basic.auto_commit_unique;
        if ui.checkbox(&mut auto, "四码唯一时自动上屏").changed() {
            state.config.basic.auto_commit_unique = auto;
            state.mark_dirty();
        }
    });
    ui.horizontal(|ui| {
        let mut commit = state.config.basic.commit_on_max_code_overflow;
        if ui
            .checkbox(&mut commit, "超过最大码长时顶首选并开始下一码")
            .changed()
        {
            state.config.basic.commit_on_max_code_overflow = commit;
            state.mark_dirty();
        }
    });
    ui.horizontal(|ui| {
        let mut show = state.config.basic.show_code_hints;
        if ui
            .checkbox(&mut show, "候选编码不全时显示后续编码提示")
            .changed()
        {
            state.config.basic.show_code_hints = show;
            state.mark_dirty();
        }
    });

    ui.horizontal(|ui| {
        ui.label("当前码表万能键:");
        if let Some(error) = &state.table_editor.load_error {
            ui.colored_label(
                eframe::egui::Color32::LIGHT_RED,
                format!("读取失败：{error}"),
            );
        } else if let Some(wildcard) = state.table_editor.config.wildcard_key {
            ui.monospace(wildcard.to_string());
        } else {
            ui.label("未启用");
        }
    });
    ui.small("万能键由当前码表 YAML 头配置，请在“码表与词库”面板编辑。");

    // 标点输入
    ui.horizontal(|ui| {
        ui.label("标点输入:");
        let mut mode = state.config.basic.punctuation_mode;
        let resp = eframe::egui::ComboBox::from_id_source("punctuation_mode")
            .selected_text(punctuation_mode_label(mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut mode,
                    PunctuationMode::BufferedCommit,
                    "加入缓冲，最后一起上屏",
                );
                ui.selectable_value(
                    &mut mode,
                    PunctuationMode::DirectCommit,
                    "立即上屏，不进入编码",
                );
            });
        if resp.response.changed() && mode != state.config.basic.punctuation_mode {
            state.config.basic.punctuation_mode = mode;
            state.mark_dirty();
        }
    });

    // 翻页热键
    ui.separator();
    ui.label("快速选词热键:");
    ui.small("候选列表显示时，可直接上屏第 2 项和第 3 项。");
    ui.horizontal(|ui| {
        ui.label("第二候选:");
        let mut select_second = state.config.hotkey.select_second.clone();
        hotkey_combo(ui, "select_second", &mut select_second);
        apply_hotkey_change(
            &mut state.config.hotkey.select_second,
            select_second,
            &mut state.dirty,
            &mut state.status_msg,
        );
    });
    ui.horizontal(|ui| {
        ui.label("第三候选:");
        let mut select_third = state.config.hotkey.select_third.clone();
        hotkey_combo(ui, "select_third", &mut select_third);
        apply_hotkey_change(
            &mut state.config.hotkey.select_third,
            select_third,
            &mut state.dirty,
            &mut state.status_msg,
        );
    });

    ui.separator();
    ui.label("翻页热键:");
    ui.horizontal(|ui| {
        ui.label("下一页:");
        let mut next = state.config.hotkey.page_next.clone();
        hotkey_combo(ui, "page_next", &mut next);
        apply_hotkey_change(
            &mut state.config.hotkey.page_next,
            next,
            &mut state.dirty,
            &mut state.status_msg,
        );
    });
    ui.horizontal(|ui| {
        ui.label("上一页:");
        let mut prev = state.config.hotkey.page_prev.clone();
        hotkey_combo(ui, "page_prev", &mut prev);
        apply_hotkey_change(
            &mut state.config.hotkey.page_prev,
            prev,
            &mut state.dirty,
            &mut state.status_msg,
        );
    });
}

fn commit_mode_label(m: CommitMode) -> &'static str {
    match m {
        CommitMode::SpaceFirst => "空格首选上屏",
        CommitMode::EnterCommit => "回车上屏",
    }
}

fn switch_key_label(k: SwitchKey) -> &'static str {
    match k {
        SwitchKey::Shift => "Shift",
        SwitchKey::CapsLock => "CapsLock",
        SwitchKey::CtrlSpace => "Ctrl+Space",
    }
}

fn punctuation_mode_label(m: PunctuationMode) -> &'static str {
    match m {
        PunctuationMode::BufferedCommit => "加入缓冲，最后一起上屏",
        PunctuationMode::DirectCommit => "立即上屏，不进入编码",
    }
}

fn hotkey_combo(ui: &mut Ui, id_source: &str, value: &mut String) {
    eframe::egui::ComboBox::from_id_source(id_source)
        .selected_text(hotkey_label(value))
        .show_ui(ui, |ui| {
            for (key, label) in HOTKEY_OPTIONS {
                ui.selectable_value(value, key.to_string(), label);
            }
        });
}

fn apply_hotkey_change(
    current: &mut String,
    pending: String,
    dirty: &mut bool,
    status_msg: &mut Option<String>,
) -> bool {
    if *current == pending {
        return false;
    }

    *current = pending;
    crate::state::set_dirty(dirty, status_msg);
    true
}

fn hotkey_label(k: &str) -> &'static str {
    match k {
        "comma" => "逗号 (,)",
        "period" => "句号 (.)",
        "semicolon" => "分号 (;)",
        "quote" => "单引号 (')",
        "minus" => "减号 (-)",
        "equal" => "等号 (=)",
        "space" => "空格",
        "left" => "左箭头",
        "right" => "右箭头",
        "page_down" => "PageDown",
        "page_up" => "PageUp",
        _ => "未知",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_hotkey_change_updates_value_and_marks_dirty() {
        let mut current = "comma".to_string();
        let mut dirty = false;
        let mut status_msg = Some("已保存".to_string());

        let changed = apply_hotkey_change(
            &mut current,
            "period".to_string(),
            &mut dirty,
            &mut status_msg,
        );

        assert!(changed);
        assert_eq!(current, "period");
        assert!(dirty);
        assert_eq!(status_msg, None);
    }
}
