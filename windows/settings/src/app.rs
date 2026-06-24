//! SettingsApp：eframe::App 实现，编排侧边栏 + 面板 + 保存栏。

use crate::save;
use crate::state::{AppState, Panel};
use eframe::egui;

pub struct SettingsApp {
    pub state: AppState,
    /// 关闭确认对话框是否显示。
    close_confirm: bool,
}

impl SettingsApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: AppState) -> Self {
        crate::fonts::load_chinese_fonts(&cc.egui_ctx);
        Self {
            state,
            close_confirm: false,
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
                    save::save(&mut self.state);
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
                            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
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