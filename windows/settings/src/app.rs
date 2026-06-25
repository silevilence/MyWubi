//! SettingsApp：eframe::App 实现，编排侧边栏 + 面板 + 保存栏。

use crate::save;
use crate::state::{AppState, FilePickTarget, Panel};
use eframe::egui;

pub struct SettingsApp {
    pub state: AppState,
    /// 关闭确认对话框是否显示。
    close_confirm: bool,
    /// 保存失败时弹模态框（独立于 status_msg）。
    save_error: Option<String>,
}

impl SettingsApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: AppState) -> Self {
        crate::fonts::load_chinese_fonts(&cc.egui_ctx);
        Self {
            state,
            close_confirm: false,
            save_error: None,
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show_sidebar(ctx);
        self.show_main_panel(ctx);
        self.poll_rfd_pick(ctx);
        self.update_title(ctx);
        self.handle_close_request(ctx);
        self.show_close_confirm(ctx);
        self.show_save_error_modal(ctx);
        self.show_load_error_modal(ctx);
    }
}

impl SettingsApp {
    fn show_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav")
            .resizable(false)
            .default_width(160.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.heading("MyWubi 设置");
                ui.add_space(12.0);
                nav_item(ui, &mut self.state, Panel::Basic, "常规设置");
                nav_item(ui, &mut self.state, Panel::Appearance, "外观样式");
                nav_item(ui, &mut self.state, Panel::Dictionary, "码表与词库");
                nav_item(ui, &mut self.state, Panel::TipManager, "输入法管理");
                nav_item(ui, &mut self.state, Panel::About, "关于");
            });
    }

    fn show_main_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            crate::panels::show_active(ui, &mut self.state);
            ui.add_space(16.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("保存").clicked() {
                    if !save::save(&mut self.state) {
                        self.save_error = self.state.status_msg.clone();
                    }
                }
                if ui.button("重新加载").clicked() {
                    self.state = AppState::load(self.state.config_path.clone());
                }
                if let Some(msg) = &self.state.status_msg {
                    ui.label(msg);
                }
            });
        });
    }

    fn poll_rfd_pick(&mut self, ctx: &egui::Context) {
        if let Some(ref pick) = self.state.pending_pick {
            if let Ok(result) = pick.rx.try_recv() {
                let target = pick.target;
                self.state.pending_pick = None;
                if let Some(path) = result {
                    match target {
                        FilePickTarget::SystemTableDir => {
                            self.state.table_dir = path;
                            self.state.rescan_tables();
                            self.state.mark_dirty();
                        }
                        FilePickTarget::UserTable => {
                            self.state.config.dictionary.user_table = path;
                        }
                    }
                    self.state.mark_dirty();
                }
            } else {
                ctx.request_repaint();
            }
        }
    }

    fn update_title(&self, ctx: &egui::Context) {
        let title = if self.state.dirty { "MyWubi 设置 *" } else { "MyWubi 设置" };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.into()));
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && self.state.dirty && !self.close_confirm {
            self.close_confirm = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    fn show_close_confirm(&mut self, ctx: &egui::Context) {
        if !self.close_confirm { return; }
        egui::Window::new("未保存的改动")
            .collapsible(false).resizable(false)
            .show(ctx, |ui| {
                ui.label("有未保存的配置改动，是否保存？");
                if let Some(msg) = &self.state.status_msg {
                    ui.colored_label(egui::Color32::LIGHT_RED, msg);
                }
                ui.horizontal(|ui| {
                    if ui.button("保存").clicked() {
                        if save::save(&mut self.state) {
                            self.close_confirm = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    if ui.button("不保存").clicked() {
                        self.close_confirm = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("取消").clicked() {
                        self.close_confirm = false;
                    }
                });
            });
    }

    fn show_save_error_modal(&mut self, ctx: &egui::Context) {
        if let Some(ref err) = self.save_error {
            let err = err.clone(); // clone once for the modal, then clear
            self.save_error = None;
            egui::Window::new("保存失败")
                .collapsible(false).resizable(false)
                .show(ctx, |ui| {
                    ui.label(&err);
                    if ui.button("确定").clicked() {
                        // error already cleared above
                    }
                });
        }
    }

    fn show_load_error_modal(&mut self, ctx: &egui::Context) {
        if self.state.load_error.is_none() { return; }
        egui::Window::new("配置加载失败")
            .collapsible(false).resizable(false)
            .show(ctx, |ui| {
                let err = self.state.load_error.clone().unwrap();
                ui.label(format!("配置文件解析失败：{}", err.message));
                ui.label(format!("路径：{}", err.path.display()));
                ui.add_space(8.0);
                ui.label("可选择加载默认配置覆盖，或打开文件位置自行修复。");
                ui.horizontal(|ui| {
                    if ui.button("加载默认配置并覆盖").clicked() {
                        self.state.apply_default_overwrite();
                    }
                    if ui.button("打开文件位置").clicked() {
                        #[cfg(windows)]
                        {
                            use std::process::Command;
                            let path_str = err.path.display().to_string();
                            let _ = Command::new("explorer")
                                .args(["/select,", &path_str]).spawn();
                        }
                    }
                    if ui.button("忽略（用默认配置，不覆盖）").clicked() {
                        self.state.load_error = None;
                        self.state.status_msg = Some(
                            "[!] 配置加载失败，当前使用默认配置（原文件未改动）".into()
                        );
                    }
                });
            });
    }
}

fn nav_item(ui: &mut egui::Ui, state: &mut AppState, panel: Panel, label: &str) {
    let selected = state.active_panel == panel;
    if ui.selectable_label(selected, label).clicked() {
        state.active_panel = panel;
        state.uninstall_confirm = false;
    }
}