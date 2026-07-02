//! 码表与词库面板。

use crate::state::{self, AppState, FilePickTarget, PickRequest};
use core_engine::{Entry, UserDictionary};
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
                state
                    .config_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            };
            std::thread::spawn(move || {
                let _ = tx.send(
                    rfd::FileDialog::new()
                        .set_directory(&start_dir)
                        .pick_folder(),
                );
            });
            state.pending_pick = Some(PickRequest {
                target: FilePickTarget::SystemTableDir,
                rx,
            });
        }
    });

    // ── 码表文件选择 ──
    if !state.scanned_tables.is_empty() {
        ui.horizontal(|ui| {
            ui.label("使用码表:");
            let current = state
                .config
                .dictionary
                .system_table
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("");
            let current_idx = state
                .scanned_tables
                .iter()
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
                if state.table_editor.dirty {
                    state.status_msg = Some("[!] 请先保存当前码表改动".into());
                } else {
                    let path = state.table_dir.join(&state.scanned_tables[idx]);
                    state.config.dictionary.system_table = path.clone();
                    state.table_editor = crate::state::TableEditor::load(path);
                    state.mark_dirty();
                }
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

    ui.horizontal(|ui| {
        if ui.button("管理当前码表…").clicked() {
            state.table_editor.open = true;
        }
        ui.label(format!(
            "当前码表共 {} 条",
            state.table_editor.entries.len()
        ));
    });

    ui.separator();

    // ── 用户词库路径 ──
    let base_dir = state
        .config_path
        .parent()
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
        open_user_dictionary(state);
    }

    show_table_editor_window(ui.ctx(), state);
    show_user_dictionary_window(ui.ctx(), state);
}

fn show_table_editor_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.table_editor.open {
        return;
    }

    let mut open = true;
    egui::Window::new("管理当前码表")
        .open(&mut open)
        .default_width(900.0)
        .default_height(650.0)
        .min_height(420.0)
        .resizable(true)
        .vscroll(true)
        .show(ctx, |ui| show_table_editor(ui, state));
    state.table_editor.open = open;
}

fn show_table_editor(ui: &mut Ui, state: &mut AppState) {
    if let Some(error) = &state.table_editor.load_error {
        ui.colored_label(egui::Color32::LIGHT_RED, format!("码表读取失败：{error}"));
        return;
    }

    let mut selected = None;
    let mut update_entry = false;
    let mut save_as = false;
    let mut save_now = false;
    let mut validate_now = false;
    let mut changed = false;
    {
        let editor = &mut state.table_editor;
        ui.label(format!(
            "{}（{} 条）",
            editor.path.display(),
            editor.entries.len()
        ));

        ui.horizontal(|ui| {
            ui.label("万能键:");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut editor.wildcard_key)
                        .desired_width(48.0)
                        .hint_text("留空禁用"),
                )
                .changed();
            ui.label("编码字符集:");
            changed |= ui
                .add(egui::TextEdit::singleline(&mut editor.config.charset).desired_width(260.0))
                .changed();
            validate_now = ui.button("验证码表").clicked();
        });
        ui.collapsing("YAML 头预览", |ui| {
            let wildcard = editor.wildcard_key.trim();
            ui.monospace(format!(
                "---\nwildcard_key: {wildcard:?}\ncharset: {:?}\n---",
                editor.config.charset,
            ));
        });

        ui.horizontal(|ui| {
            ui.label("搜索:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut editor.search)
                    .desired_width(260.0)
                    .hint_text("输入字词或编码"),
            );
            // ponytail: 百万词条只在提交时线性扫描；实测不够快再加索引。
            if ui.button("搜索").clicked()
                || (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
            {
                editor.refresh_filter();
            }
            if ui.button("清除").clicked() {
                editor.search.clear();
                editor.refresh_filter();
            }
            ui.label(format!("显示 {} 条", editor.visible_len()));
        });

        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("编码");
            ui.add_space(116.0);
            ui.strong("字词");
            ui.add_space(166.0);
            ui.strong("权重");
        });
        let row_height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        let list_height = (ui.available_height() - 125.0).max(180.0);
        egui::ScrollArea::vertical()
            .max_height(list_height)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show_rows(ui, row_height, editor.visible_len(), |ui, rows| {
                for row in rows {
                    let index = editor.visible_entry_index(row);
                    let entry = &editor.entries[index];
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized(
                                [150.0, row_height],
                                egui::SelectableLabel::new(
                                    editor.selected == Some(index),
                                    &entry.code,
                                ),
                            )
                            .clicked()
                        {
                            selected = Some(index);
                        }
                        ui.add_sized([200.0, row_height], egui::Label::new(&entry.word));
                        ui.label(entry.weight.to_string());
                    });
                }
            });

        if let Some(index) = selected {
            editor.select(index);
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("编码");
            ui.add(egui::TextEdit::singleline(&mut editor.code).desired_width(100.0));
            ui.label("字词");
            ui.add(egui::TextEdit::singleline(&mut editor.word).desired_width(140.0));
            ui.label("权重");
            ui.add(egui::DragValue::new(&mut editor.weight));
            update_entry = ui
                .add_enabled(editor.selected.is_some(), egui::Button::new("更新词条"))
                .clicked();
        });

        ui.horizontal(|ui| {
            ui.label("另存为:");
            ui.add(
                egui::TextEdit::singleline(&mut editor.save_as_name)
                    .desired_width(180.0)
                    .hint_text("自定义码表名称"),
            );
            save_as = ui.button("保存到码表目录").clicked();
            save_now = ui
                .add_enabled(editor.dirty, egui::Button::new("保存当前码表"))
                .clicked();
        });
    }

    if changed {
        state.table_editor.dirty = true;
        state.table_editor.validation_stale = true;
        state.mark_dirty();
    }
    if validate_now {
        state.table_editor.refresh_validation();
        state.status_msg = Some(if state.table_editor.validation.is_valid() {
            "[OK] 码表校验通过".into()
        } else {
            format!(
                "[ERR] 发现 {} 个码表违规项",
                state.table_editor.validation.issue_count
            )
        });
    }
    if update_entry {
        match state.table_editor.update_selected() {
            Ok(()) => {
                state.mark_dirty();
                state.status_msg = Some("[OK] 已更新词条，等待保存".into());
            }
            Err(error) => state.status_msg = Some(format!("[ERR] {error}")),
        }
    }
    if save_as {
        save_table_as(state);
    }
    if save_now {
        crate::save::save(state);
    }

    if let Err(error) = state.table_editor.draft_config() {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    } else if state.table_editor.validation_stale {
        ui.colored_label(egui::Color32::YELLOW, "配置已修改，请点击“验证码表”");
    } else {
        show_validation_report(ui, &state.table_editor.validation);
    }
}

fn show_validation_report(ui: &mut Ui, report: &core_engine::TableValidationReport) {
    if report.is_valid() {
        ui.colored_label(egui::Color32::LIGHT_GREEN, "校验通过");
        return;
    }

    ui.colored_label(
        egui::Color32::LIGHT_RED,
        format!("发现 {} 个违规项", report.issue_count),
    );
    egui::ScrollArea::vertical()
        .max_height(100.0)
        .show(ui, |ui| {
            for issue in &report.issues {
                let location = issue
                    .entry_index
                    .map_or_else(|| "YAML 头".into(), |index| format!("词条 {}", index + 1));
                ui.label(format!("{location}: {}", issue.message));
            }
            if report.issue_count > report.issues.len() {
                ui.label(format!(
                    "另有 {} 项未显示",
                    report.issue_count - report.issues.len()
                ));
            }
        });
}

fn save_table_as(state: &mut AppState) {
    let name = state.table_editor.save_as_name.trim();
    if name.is_empty() {
        state.status_msg = Some("[ERR] 请输入自定义码表名称".into());
        return;
    }
    if !matches!(
        Path::new(name).components().next(),
        Some(std::path::Component::Normal(_))
    ) || Path::new(name).components().count() != 1
    {
        state.status_msg = Some("[ERR] 码表名称不能包含路径".into());
        return;
    }

    let file_name = if name.ends_with(".dict") {
        name.to_string()
    } else {
        format!("{name}.dict")
    };
    let path = state.table_dir.join(file_name);
    if path.exists() {
        state.status_msg = Some(format!("[ERR] {} 已存在", path.display()));
        return;
    }

    match state.table_editor.save_to(path.clone()) {
        Ok(()) => {
            state.config.dictionary.system_table = path.clone();
            state.rescan_tables();
            state.mark_dirty();
            state.status_msg = Some(format!("[OK] 已另存为 {}", path.display()));
        }
        Err(error) => state.status_msg = Some(format!("[ERR] 另存失败: {error}")),
    }
}

fn open_user_dictionary(state: &mut AppState) {
    let path = crate::config_path::resolve_resource_path(
        &state.config_path,
        &state.config.dictionary.user_table,
    );
    match UserDictionary::load(&path) {
        Ok(dictionary) => {
            state.user_dictionary_editor.dictionary = Some(dictionary);
            state.user_dictionary_editor.clear_form();
            state.user_dictionary_editor.open = true;
            state.status_msg = Some(format!("[OK] 已打开用户词库 {}", path.display()));
        }
        Err(error) => state.status_msg = Some(format!("[ERR] {error}")),
    }
}

fn show_user_dictionary_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.user_dictionary_editor.open {
        return;
    }

    let mut open = true;
    let mut requested_pick = None;
    let mut action = None;
    egui::Window::new("管理自造词")
        .open(&mut open)
        .default_width(560.0)
        .show(ctx, |ui| {
            let editor = &mut state.user_dictionary_editor;
            let Some(dictionary) = editor.dictionary.as_mut() else {
                ui.label("用户词库未加载");
                return;
            };

            ui.label(format!(
                "{}（{} 条）",
                dictionary.path().display(),
                dictionary.entries().len()
            ));
            ui.horizontal(|ui| {
                if ui.button("导入…").clicked() && state.pending_pick.is_none() {
                    requested_pick = Some((
                        FilePickTarget::UserDictionaryImport,
                        dictionary.path().to_path_buf(),
                    ));
                }
                if ui.button("导出…").clicked() && state.pending_pick.is_none() {
                    requested_pick = Some((
                        FilePickTarget::UserDictionaryExport,
                        dictionary.path().to_path_buf(),
                    ));
                }
            });
            ui.separator();

            let mut selected = None;
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    egui::Grid::new("user_dictionary_entries")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("编码");
                            ui.strong("词条");
                            ui.strong("词频");
                            ui.end_row();
                            for (index, entry) in dictionary.entries().iter().enumerate() {
                                if ui
                                    .selectable_label(editor.selected == Some(index), &entry.code)
                                    .clicked()
                                {
                                    selected = Some(index);
                                }
                                ui.label(&entry.word);
                                ui.label(entry.weight.to_string());
                                ui.end_row();
                            }
                        });
                });

            if let Some(index) = selected {
                let entry = &dictionary.entries()[index];
                editor.code.clone_from(&entry.code);
                editor.word.clone_from(&entry.word);
                editor.weight = entry.weight;
                editor.selected = Some(index);
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("编码");
                ui.text_edit_singleline(&mut editor.code);
                ui.label("词条");
                ui.text_edit_singleline(&mut editor.word);
                ui.label("词频");
                ui.add(egui::DragValue::new(&mut editor.weight));
            });
            ui.horizontal(|ui| {
                if ui.button("新增").clicked() {
                    action = Some(UserDictionaryAction::Add(Entry {
                        code: editor.code.trim().to_string(),
                        word: editor.word.trim().to_string(),
                        weight: editor.weight,
                    }));
                }
                if ui
                    .add_enabled(editor.selected.is_some(), egui::Button::new("更新"))
                    .clicked()
                {
                    action = Some(UserDictionaryAction::Update(
                        editor.selected.unwrap_or_default(),
                        Entry {
                            code: editor.code.trim().to_string(),
                            word: editor.word.trim().to_string(),
                            weight: editor.weight,
                        },
                    ));
                }
                if ui
                    .add_enabled(editor.selected.is_some(), egui::Button::new("删除"))
                    .clicked()
                {
                    action = Some(UserDictionaryAction::Remove(
                        editor.selected.unwrap_or_default(),
                    ));
                }
                if ui.button("清空表单").clicked() {
                    editor.clear_form();
                }
            });
        });
    state.user_dictionary_editor.open = open;
    if let Some(action) = action {
        apply_user_dictionary_action(state, action);
    }
    if let Some((target, path)) = requested_pick {
        start_dictionary_pick(state, target, &path);
    }
}

enum UserDictionaryAction {
    Add(Entry),
    Update(usize, Entry),
    Remove(usize),
}

fn apply_user_dictionary_action(state: &mut AppState, action: UserDictionaryAction) {
    let editor = &mut state.user_dictionary_editor;
    let Some(dictionary) = editor.dictionary.as_mut() else {
        state.status_msg = Some("[ERR] 用户词库尚未打开".into());
        return;
    };
    let (result, success) = match action {
        UserDictionaryAction::Add(entry) => (dictionary.add(entry), "已新增词条"),
        UserDictionaryAction::Update(index, entry) => {
            (dictionary.update(index, entry), "已更新词条")
        }
        UserDictionaryAction::Remove(index) => (dictionary.remove(index).map(|_| ()), "已删除词条"),
    };
    match result {
        Ok(()) => {
            editor.clear_form();
            state.status_msg = Some(format!("[OK] {success}，输入法将自动热重载"));
        }
        Err(error) => state.status_msg = Some(format!("[ERR] {error}")),
    }
}

fn start_dictionary_pick(state: &mut AppState, target: FilePickTarget, path: &Path) {
    let (tx, rx) = std::sync::mpsc::channel();
    let directory = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    std::thread::spawn(move || {
        let dialog = rfd::FileDialog::new()
            .set_directory(directory)
            .add_filter("MyWubi 用户词库", &["dict"]);
        let result = match target {
            FilePickTarget::UserDictionaryExport => dialog.set_file_name("user.dict").save_file(),
            _ => dialog.pick_file(),
        };
        let _ = tx.send(result);
    });
    state.pending_pick = Some(PickRequest { target, rx });
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

fn path_row(
    ui: &mut Ui,
    label: &str,
    path: &mut PathBuf,
    dirty: &mut bool,
    status_msg: &mut Option<String>,
    pending: &mut Option<PickRequest>,
    target: FilePickTarget,
    base_dir: &Path,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut s = path.display().to_string();
        if ui.text_edit_singleline(&mut s).changed() {
            *path = PathBuf::from(s);
            state::set_dirty(dirty, status_msg);
        }
        if ui.button("浏览…").clicked() && pending.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let start_dir = path
                .parent()
                .filter(|p| p.exists())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| base_dir.to_path_buf());
            std::thread::spawn(move || {
                let dialog = rfd::FileDialog::new().set_directory(&start_dir);
                let result = match target {
                    FilePickTarget::UserTable => dialog.set_file_name("user.dict").save_file(),
                    _ => dialog.pick_file(),
                };
                let _ = tx.send(result);
            });
            *pending = Some(PickRequest { target, rx });
        }
    });
}
