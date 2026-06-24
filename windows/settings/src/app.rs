//! SettingsApp：eframe::App 实现，编排侧边栏 + 面板 + 保存栏。

use crate::save;
use crate::state::{AppState, Panel};
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
        // 侧边栏
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
                nav_item(ui, &mut self.state, Panel::About, "关于");
            });

        // 主区域
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

        // 标题栏未保存标记
        let title = if self.state.dirty {
            "MyWubi 设置 *"
        } else {
            "MyWubi 设置"
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.into()));

        // 检测窗口关闭请求：若有未保存改动，拦截并弹确认框
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && self.state.dirty && !self.close_confirm {
            self.close_confirm = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        // 关闭确认对话框
        if self.close_confirm {
            egui::Window::new("未保存的改动")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("有未保存的配置改动，是否保存？");
                    // 保存失败时在模态内显示错误
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

        // 保存失败模态弹窗（spec §8 要求）
        if let Some(err) = self.save_error.clone() {
            egui::Window::new("保存失败")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(err);
                    if ui.button("确定").clicked() {
                        self.save_error = None;
                    }
                });
        }

        // 启动期配置加载错误对话框（用户确认后才覆盖损坏文件）
        if self.state.load_error.is_some() {
            egui::Window::new("配置加载失败")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    // 先克隆错误信息，避免与后续 self.state 可变借用冲突
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
                                    .args(["/select,", &path_str])
                                    .spawn();
                            }
                        }
                        if ui.button("忽略（用默认配置，不覆盖）").clicked() {
                            self.state.load_error = None;
                            self.state.status_msg = Some(
                                "⚠️ 配置加载失败，当前使用默认配置（原文件未改动）".into(),
                            );
                        }
                    });
                });
        }
    }
}

fn nav_item(ui: &mut egui::Ui, state: &mut AppState, panel: Panel, label: &str) {
    let selected = state.active_panel == panel;
    if ui.selectable_label(selected, label).clicked() {
        state.active_panel = panel;
    }
}