//! 常规设置面板。

use crate::state::AppState;
use core_engine::config::{CommitMode, PunctuationMode, SwitchKey};
use eframe::egui::Ui;

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
        ui.label("标点输入:");
        let mut mode = state.config.basic.punctuation_mode;
        let resp = eframe::egui::ComboBox::from_id_source("punctuation_mode")
            .selected_text(punctuation_mode_label(mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, PunctuationMode::BufferedCommit, "加入缓冲，最后一起上屏");
                ui.selectable_value(&mut mode, PunctuationMode::DirectCommit, "立即上屏，不进入编码");
            });
        if resp.response.changed() && mode != state.config.basic.punctuation_mode {
            state.config.basic.punctuation_mode = mode;
            state.mark_dirty();
        }
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