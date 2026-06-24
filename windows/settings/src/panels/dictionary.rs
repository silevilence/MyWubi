//! 码表与词库面板。

use crate::state::{AppState, FilePickTarget, PickRequest};
use eframe::egui::Ui;
use std::path::PathBuf;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("码表与词库");
    ui.separator();

    path_row(
        ui,
        "系统码表路径:",
        &mut state.config.dictionary.system_table,
        &mut state.dirty,
        &mut state.status_msg,
        &mut state.pending_pick,
        FilePickTarget::SystemTable,
    );
    path_row(
        ui,
        "用户词库路径:",
        &mut state.config.dictionary.user_table,
        &mut state.dirty,
        &mut state.status_msg,
        &mut state.pending_pick,
        FilePickTarget::UserTable,
    );

    ui.separator();

    ui.horizontal(|ui| {
        let mut exact = state.config.dictionary.enable_exact_match;
        if ui.checkbox(&mut exact, "启用精确匹配优先").changed() {
            state.config.dictionary.enable_exact_match = exact;
            state.mark_dirty();
        }
    });

    ui.horizontal(|ui| {
        let mut fuzzy = state.config.dictionary.enable_fuzzy;
        if ui.checkbox(&mut fuzzy, "启用模糊音").changed() {
            state.config.dictionary.enable_fuzzy = fuzzy;
            state.mark_dirty();
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        let mut user_dict = state.config.dictionary.enable_user_dict;
        if ui.checkbox(&mut user_dict, "启用用户词库功能").changed() {
            state.config.dictionary.enable_user_dict = user_dict;
            state.mark_dirty();
        }
    });

    if ui.button("管理自造词…").clicked() {
        state.status_msg = Some("ℹ️ 用户词库管理功能待开发".into());
    }
}

fn path_row(ui: &mut Ui, label: &str, path: &mut PathBuf, dirty: &mut bool, status_msg: &mut Option<String>, pending: &mut Option<PickRequest>, target: FilePickTarget) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut s = path.display().to_string();
        if ui.text_edit_singleline(&mut s).changed() {
            *path = PathBuf::from(s);
            *dirty = true;
            *status_msg = None;
        }
        if ui.button("浏览…").clicked() && pending.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(rfd::FileDialog::new().pick_file());
            });
            *pending = Some(PickRequest { target, rx });
        }
    });
}