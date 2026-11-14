//! SettingsApp：eframe::App 实现，编排侧边栏 + 面板 + 保存栏。

use crate::save;
use crate::state::{AppState, FilePickTarget, Panel};
use eframe::egui;

pub struct SettingsApp {
    pub state: AppState,
    /// 关闭确认对话框是否显示。
    close_confirm: bool,
    allow_close: bool,
    reload_confirm: bool,
    save_requested: bool,
    /// 保存失败时弹模态框（独立于 status_msg）。
    save_error: Option<String>,
}

impl SettingsApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: AppState) -> Self {
        crate::fonts::load_chinese_fonts(&cc.egui_ctx);
        crate::ui_style::install(&cc.egui_ctx);
        Self {
            state,
            close_confirm: false,
            allow_close: false,
            reload_confirm: false,
            save_requested: false,
            save_error: None,
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S))
            && !self.close_confirm
            && !self.reload_confirm
            && self.state.load_error.is_none()
        {
            self.save_requested = true;
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        self.show_sidebar(ctx);
        self.show_main_panel(ctx);
        if self.save_requested {
            self.save_requested = false;
            self.save_now();
        }
        self.poll_rfd_pick(ctx);
        self.poll_update_worker(ctx);
        self.update_title(ctx);
        self.handle_close_request(ctx);
        self.show_close_confirm(ctx);
        self.show_reload_confirm(ctx);
        self.show_save_error_modal(ctx);
        self.show_load_error_modal(ctx);
    }
}

impl SettingsApp {
    fn save_now(&mut self) {
        if !save::save(&mut self.state) {
            self.save_error = self.state.status_msg.clone();
        }
    }

    fn show_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(184.0)
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(235, 239, 244))
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.set_enabled(
                    !self.close_confirm
                        && !self.reload_confirm
                        && self.save_error.is_none()
                        && self.state.load_error.is_none(),
                );
                ui.add_space(8.0);
                ui.label(egui::RichText::new("MyWubi").size(25.0).strong());
                ui.label(egui::RichText::new("输入法设置").color(crate::ui_style::MUTED));
                ui.add_space(28.0);
                for (panel, label) in [
                    (Panel::Basic, "常用输入"),
                    (Panel::Appearance, "候选外观"),
                    (Panel::Dictionary, "码表与词库"),
                    (Panel::SpecialInput, "特殊输入"),
                    (Panel::Intelligence, "预测与学习"),
                    (Panel::TipManager, "输入法管理"),
                    (Panel::About, "关于与更新"),
                ] {
                    nav_item(ui, &mut self.state, panel, label);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.small(format!("版本 {}", env!("CARGO_PKG_VERSION")));
                    ui.small(if self.state.portable {
                        "便携模式"
                    } else {
                        "用户配置"
                    });
                });
            });
    }

    fn show_main_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("save-actions")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.set_enabled(
                    !self.close_confirm
                        && !self.reload_confirm
                        && self.save_error.is_none()
                        && self.state.load_error.is_none(),
                );
                ui.horizontal(|ui| {
                    let dirty = self.state.has_unsaved_changes();
                    ui.label(
                        egui::RichText::new(if dirty {
                            "有未保存的修改"
                        } else {
                            "所有更改已保存"
                        })
                        .color(crate::ui_style::MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::ui_style::primary(
                            ui,
                            "保存更改",
                            dirty && self.state.load_error.is_none(),
                        )
                        .on_hover_text("Ctrl+S · 保存后自动生效")
                        .clicked()
                        {
                            self.save_requested = true;
                        }
                        if ui
                            .button("重新加载")
                            .on_hover_text("从磁盘读取最新配置")
                            .clicked()
                        {
                            if dirty {
                                self.reload_confirm = true;
                            } else if let Err(e) = self.state.reload(false) {
                                self.save_error = Some(e);
                            }
                        }
                    });
                });
                if let Some(msg) = &self.state.status_msg {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(msg).color(if msg.starts_with("[ERR]") {
                        crate::ui_style::ERROR
                    } else {
                        crate::ui_style::MUTED
                    }));
                }
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(crate::ui_style::BACKGROUND)
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| {
                ui.set_enabled(
                    !self.close_confirm
                        && !self.reload_confirm
                        && self.save_error.is_none()
                        && self.state.load_error.is_none(),
                );
                if self.state.active_panel == Panel::SpecialInput {
                    ui.push_id(self.state.ui_revision, |ui| {
                        crate::panels::show_active(ui, &mut self.state)
                    });
                } else {
                    egui::ScrollArea::vertical()
                        .id_source(("settings-page", format!("{:?}", self.state.active_panel)))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.push_id(self.state.ui_revision, |ui| {
                                crate::panels::show_active(ui, &mut self.state)
                            });
                        });
                }
            });
    }

    fn show_reload_confirm(&mut self, ctx: &egui::Context) {
        if !self.reload_confirm {
            return;
        }
        egui::Window::new("放弃修改并重新加载？")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("尚未保存的配置和码表修改将被丢弃。");
                ui.horizontal(|ui| {
                    if ui.button("继续编辑").clicked() {
                        self.reload_confirm = false;
                    }
                    if ui.button("放弃并重新加载").clicked() {
                        self.reload_confirm = false;
                        if let Err(e) = self.state.reload(true) {
                            self.save_error = Some(e);
                        }
                    }
                });
            });
    }

    fn poll_rfd_pick(&mut self, ctx: &egui::Context) {
        if let Some(ref pick) = self.state.pending_pick {
            let received = pick.rx.try_recv();
            if let Ok(result) = received {
                let target = pick.target;
                self.state.pending_pick = None;
                if let Some(path) = result {
                    match target {
                        FilePickTarget::SystemTableDir => {
                            self.state.browse_tables(path);
                            return;
                        }
                        FilePickTarget::UserTable => {
                            self.state.config.dictionary.user_table = path;
                        }
                        FilePickTarget::UserDictionaryImport => {
                            let result = self
                                .state
                                .user_dictionary_editor
                                .dictionary
                                .as_mut()
                                .ok_or_else(|| "用户词库尚未打开".to_string())
                                .and_then(|dictionary| {
                                    dictionary
                                        .import(&path)
                                        .map(|count| format!("[OK] 已导入 {count} 个新词条"))
                                        .map_err(|error| error.to_string())
                                });
                            let imported = result.is_ok();
                            self.state.status_msg =
                                Some(result.unwrap_or_else(|error| format!("[ERR] {error}")));
                            if imported {
                                self.state.prepare_app_container_access();
                            }
                            return;
                        }
                        FilePickTarget::UserDictionaryExport => {
                            let result = self
                                .state
                                .user_dictionary_editor
                                .dictionary
                                .as_ref()
                                .ok_or_else(|| "用户词库尚未打开".to_string())
                                .and_then(|dictionary| {
                                    dictionary
                                        .export(&path)
                                        .map(|()| format!("[OK] 已导出到 {}", path.display()))
                                        .map_err(|error| error.to_string())
                                });
                            self.state.status_msg =
                                Some(result.unwrap_or_else(|error| format!("[ERR] {error}")));
                            return;
                        }
                    }
                    self.state.mark_dirty();
                }
            } else if matches!(received, Err(std::sync::mpsc::TryRecvError::Disconnected)) {
                self.state.pending_pick = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }

    fn update_title(&self, ctx: &egui::Context) {
        let title = if self.state.has_unsaved_changes() {
            "MyWubi 设置 *"
        } else {
            "MyWubi 设置"
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.into()));
    }

    fn poll_update_worker(&mut self, ctx: &egui::Context) {
        use crate::vpk::{UpdateEvent, UpdateState};

        let worker = match self.state.update_worker.as_ref() {
            Some(w) => w,
            None => return,
        };

        // 一次性取出所有已到达事件，取最后一条作为最新状态。
        let mut last: Option<UpdateEvent> = None;
        while let Ok(ev) = worker.rx.try_recv() {
            last = Some(ev);
        }

        if let Some(ev) = last {
            let terminal = !matches!(&ev, UpdateEvent::DownloadProgress(_));
            self.state.update_state = match ev {
                UpdateEvent::NoUpdate => UpdateState::NoUpdate,
                UpdateEvent::Available {
                    version,
                    notes,
                    portable,
                    asset,
                } => UpdateState::Available {
                    version,
                    notes,
                    portable,
                    asset,
                },
                UpdateEvent::DownloadProgress(p) => UpdateState::Downloading { progress: p },
                UpdateEvent::DownloadDone { asset } => UpdateState::Ready { asset },
                UpdateEvent::NotInstalled => UpdateState::NotInstalled,
                UpdateEvent::Error(msg) => UpdateState::Error(msg),
            };
            // 事件处理完毕，释放 worker 句柄（线程已结束或进入空闲）。
            if terminal {
                self.state.update_worker = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        } else {
            // 仍在进行中，请求重绘以持续轮询。
            ctx.request_repaint();
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && self.state.has_unsaved_changes()
            && !self.allow_close
        {
            self.close_confirm = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    fn show_close_confirm(&mut self, ctx: &egui::Context) {
        if !self.close_confirm {
            return;
        }
        egui::Window::new("保存修改后退出？")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("配置或码表有未保存的修改。");
                ui.horizontal(|ui| {
                    if crate::ui_style::primary(ui, "保存并退出", true).clicked()
                        && save::save(&mut self.state)
                    {
                        self.allow_close = true;
                    }
                    if ui.button("放弃修改并退出").clicked() {
                        self.allow_close = true;
                    }
                    if ui.button("继续编辑").clicked() {
                        self.close_confirm = false;
                    }
                });
                if let Some(msg) = &self.state.status_msg {
                    ui.label(msg);
                }
            });
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn show_save_error_modal(&mut self, ctx: &egui::Context) {
        if let Some(err) = self.save_error.clone() {
            egui::Window::new("操作未完成")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_max_width(440.0);
                    ui.label(&err);
                    if ui.button("返回编辑").clicked() {
                        self.save_error = None;
                    }
                });
        }
    }

    fn show_load_error_modal(&mut self, ctx: &egui::Context) {
        if self.state.load_error.is_none() {
            return;
        }
        egui::Window::new("配置加载失败")
            .collapsible(false)
            .resizable(false)
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
                                .args(["/select,", &path_str])
                                .spawn();
                        }
                    }
                    if ui.button("忽略（用默认配置，不覆盖）").clicked() {
                        self.state.load_error = None;
                        self.state.status_msg =
                            Some("[!] 配置加载失败，当前使用默认配置（原文件未改动）".into());
                    }
                });
            });
    }
}

fn nav_item(ui: &mut egui::Ui, state: &mut AppState, panel: Panel, label: &str) {
    let selected = state.active_panel == panel;
    let response = ui.add_sized(
        [ui.available_width(), 38.0],
        egui::SelectableLabel::new(selected, label),
    );
    if response.clicked() {
        state.active_panel = panel;
        state.uninstall_confirm = false;
        if panel == Panel::TipManager && state.desktop_effects {
            state.tip_status = tip_manager::detect_status();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> SettingsApp {
        SettingsApp {
            state: AppState::load_isolated(std::path::PathBuf::from(
                "nonexistent-test-config.toml",
            )),
            close_confirm: false,
            allow_close: false,
            reload_confirm: false,
            save_requested: false,
            save_error: None,
        }
    }
    #[test]
    fn save_error_remains_until_user_dismisses_it() {
        let mut app = app();
        app.save_error = Some("cannot write file".into());
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |ctx| app.show_save_error_modal(ctx));
            assert!(app.save_error.is_some());
        }
    }
    #[test]
    fn update_progress_keeps_the_worker_until_a_terminal_event() {
        let mut app = app();
        let (tx, rx) = std::sync::mpsc::channel();
        app.state.update_worker = Some(crate::vpk::UpdateWorker { rx });
        tx.send(crate::vpk::UpdateEvent::DownloadProgress(25))
            .unwrap();
        app.poll_update_worker(&egui::Context::default());
        assert!(app.state.update_worker.is_some());
        tx.send(crate::vpk::UpdateEvent::NoUpdate).unwrap();
        app.poll_update_worker(&egui::Context::default());
        assert!(app.state.update_worker.is_none());
    }
}
