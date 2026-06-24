//! 码表与词库面板。

use crate::state::{self, AppState, FilePickTarget, PickRequest};
use eframe::egui::{self, Ui};
use std::path::{Path, PathBuf};

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("码表与词库");
    ui.separator();

    // ── 码表目录 ──
    ui.horizontal(|ui| {
        ui.label("码表目录:");
        let mut s = state.table_dir.display().to_string();
        if ui.text_edit_singleline(&mut s).changed() {
            let new_dir = PathBuf::from(s);
            if new_dir.is_dir() {
                state.table_dir = new_dir;
                state.rescan_tables();
                state.mark_dirty();
            }
        }
        if ui.button("浏览…").clicked() && state.pending_pick.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let start_dir = if state.table_dir.exists() {
                state.table_dir.clone()
            } else {
                state.config_path.parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            };
            std::thread::spawn(move || {
                let _ = tx.send(rfd::FileDialog::new()
                    .set_directory(&start_dir)
                    .pick_folder());
            });
            state.pending_pick = Some(PickRequest { target: FilePickTarget::SystemTableDir, rx });
        }
    });

    // ── 码表文件选择 ──
    if !state.scanned_tables.is_empty() {
        ui.horizontal(|ui| {
            ui.label("使用码表:");
            let current = state.config.dictionary.system_table
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("");
            let current_idx = state.scanned_tables.iter()
                .position(|t| t == current)
                .unwrap_or(0);
            let mut idx = current_idx;
            egui::ComboBox::from_id_source("table_select")
                .selected_text(&state.scanned_tables[current_idx])
                .show_ui(ui, |ui| {
                    for (i, name) in state.scanned_tables.iter().enumerate() {
                        ui.selectable_value(&mut idx, i, name);
                    }
                });
            if idx != current_idx {
                state.config.dictionary.system_table =
                    state.table_dir.join(&state.scanned_tables[idx]);
                state.mark_dirty();
            }
        });
    } else {
        ui.label("（目录下未找到 .dict 码表文件）");
        ui.add_space(4.0);
    }

    // ── 初始化码表：从 exe 同目录 tables/ 复制模板 ──
    ui.horizontal(|ui| {
        if ui.button("初始化码表").clicked() {
            init_tables(state);
        }
        ui.label("从程序目录 tables/ 复制模板码表到此目录");
    });

    ui.separator();

    // ── 用户词库路径 ──
    let base_dir = state.config_path.parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    path_row(
        ui,
        "用户词库路径:",
        &mut state.config.dictionary.user_table,
        &mut state.dirty,
        &mut state.status_msg,
        &mut state.pending_pick,
        FilePickTarget::UserTable,
        &base_dir,
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

/// 从 exe 同目录 `tables/` 复制所有 .dict 文件到当前 table_dir。
fn init_tables(state: &mut AppState) {
    let src = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tables")))
        .filter(|p| p.is_dir());

    let src = match src {
        Some(d) => d,
        None => {
            state.status_msg = Some("❌ 未找到程序目录下的 tables/ 模板文件夹".into());
            return;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&state.table_dir) {
        state.status_msg = Some(format!("❌ 无法创建码表目录: {e}"));
        return;
    }

    let mut copied = 0usize;
    if let Ok(entries) = std::fs::read_dir(&src) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "dict") {
                let dest = state.table_dir.join(path.file_name().unwrap());
                match std::fs::copy(&path, &dest) {
                    Ok(_) => copied += 1,
                    Err(e) => log::warn!("复制码表失败 {}: {e}", path.display()),
                }
            }
        }
    }

    state.rescan_tables();
    if copied > 0 {
        state.mark_dirty();
        state.status_msg = Some(format!("✅ 已初始化 {} 个码表文件", copied));
    } else {
        state.status_msg = Some("⚠️ tables/ 目录下未找到 .dict 文件".into());
    }
}

fn path_row(ui: &mut Ui, label: &str, path: &mut PathBuf, dirty: &mut bool, status_msg: &mut Option<String>, pending: &mut Option<PickRequest>, target: FilePickTarget, base_dir: &Path) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut s = path.display().to_string();
        if ui.text_edit_singleline(&mut s).changed() {
            *path = PathBuf::from(s);
            state::set_dirty(dirty, status_msg);
        }
        if ui.button("浏览…").clicked() && pending.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let start_dir = path.parent()
                .filter(|p| p.exists())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| base_dir.to_path_buf());
            std::thread::spawn(move || {
                let _ = tx.send(
                    rfd::FileDialog::new()
                        .set_directory(&start_dir)
                        .pick_file()
                );
            });
            *pending = Some(PickRequest { target, rx });
        }
    });
}