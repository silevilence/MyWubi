//! Standalone skin authoring window. Applying a skin reads the latest saved config.
#[cfg(test)]
mod takeover_test;
use crate::{skin_editor::SkinEditor, ui_style};
use eframe::egui;
use std::path::{Path, PathBuf};

pub fn launch(config: &Path) -> Result<(), String> {
    if crate::skin_mcp::control::signal(config, "focus") {
        return Ok(());
    }
    launch_editor(config, false)
}
pub fn toggle_mcp(config: &Path, enable: bool) -> Result<(), String> {
    if crate::skin_mcp::control::signal(config, if enable { "enable" } else { "disable" })
        || !enable
    {
        return Ok(());
    }
    launch_editor(config, true)
}
fn launch_editor(config: &Path, enable: bool) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("skin_editor.exe");
    if !executable.is_file() {
        return Err("找不到 skin_editor.exe，请重新安装完整程序包。".into());
    }
    std::process::Command::new(executable)
        .arg("--config")
        .arg(config)
        .args(enable.then_some("--enable-mcp"))
        .spawn()
        .map_err(|e| format!("无法打开皮肤编辑器：{e}"))?;
    Ok(())
}

pub struct SkinApp {
    pub editor: SkinEditor,
    config: core_engine::Config,
    path: PathBuf,
    controls: Option<crate::skin_mcp::control::Controls>,
    mcp: Option<crate::skin_mcp::Endpoint>,
    close_confirm: bool,
    allow_close: bool,
    error: Option<String>,
}

impl SkinApp {
    pub fn new(cc: &eframe::CreationContext<'_>, path: PathBuf, editor: SkinEditor) -> Self {
        crate::fonts::load_chinese_fonts(&cc.egui_ctx);
        ui_style::install(&cc.egui_ctx);
        let loaded = core_engine::Config::load(&path);
        let error = loaded
            .as_ref()
            .err()
            .map(|e| format!("无法读取配置：{e}。可以编辑皮肤，修复配置后再应用。"));
        let controls = crate::skin_mcp::control::Controls::register(&path);
        let error = controls.as_ref().err().map(|e| e.to_string()).or(error);
        Self {
            controls: controls.ok().flatten(),
            mcp: None,
            editor,
            config: loaded.unwrap_or_default(),
            path,
            close_confirm: false,
            allow_close: false,
            error,
        }
    }
    pub fn set_mcp(&mut self, enable: bool, ctx: &egui::Context) {
        if enable == self.mcp.is_some() {
            return;
        }
        let Some(controls) = &self.controls else {
            self.error = Some("请在当前配置的首个编辑器窗口接管草稿".into());
            return;
        };
        if enable {
            if self.close_confirm {
                self.error = Some("请先处理关闭草稿提示".into());
                return;
            }
            if let Err(reason) = self.editor.can_handoff() {
                self.error = Some(reason.into());
                return;
            }
            match crate::skin_mcp::Endpoint::start(ctx.clone()) {
                Ok(endpoint) => {
                    self.editor.refresh_readonly_preview();
                    self.mcp = Some(endpoint);
                }
                Err(e) => {
                    self.error = Some(format!("MCP 开启失败：{e}"));
                    return;
                }
            }
        } else {
            self.mcp = None;
        }
        if let Err(e) = controls.publish(enable) {
            self.error = Some(e.to_string());
            self.mcp = None;
        }
        ctx.request_repaint();
    }
}

impl eframe::App for SkinApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show(ctx);
    }
}
impl SkinApp {
    fn show(&mut self, ctx: &egui::Context) {
        if let Some(control) = &self.controls {
            let (enable, disable, focus) = control.poll();
            if enable || disable {
                self.set_mcp(!disable, ctx);
            }
            if focus || enable {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            ctx.request_repaint_after(crate::skin_mcp::control::POLL);
        }
        if self.mcp.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S))
            && !self.close_confirm
        {
            self.editor.export_dialog(ctx.clone());
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.mcp.is_some() || self.editor.is_dirty() || self.editor.is_busy())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.mcp.is_some() {
                self.error = Some("请先关闭 MCP，再退出编辑器".into());
            } else {
                self.close_confirm = true;
            }
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{}{} · MyWubi 皮肤编辑器",
            self.editor.title(),
            if self.editor.is_dirty() { " *" } else { "" }
        )));
        egui::TopBottomPanel::top("skin-header")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(20.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("皮肤编辑器");
                    ui.separator();
                    ui.label(self.editor.title());
                });
                if let Some(endpoint) = &self.mcp {
                    ui.horizontal_wrapped(|ui| {
                        if endpoint.active() {
                            ui.strong(format!("AI 接管中 · {}", endpoint.operation));
                        } else {
                            ui.strong("等待 Agent 接管 · 当前只读");
                        }
                        if ui.button("复制 MCP 连接地址").clicked() {
                            ui.output_mut(|o| o.copied_text = endpoint.url.clone());
                        }
                    });
                    if ui.button("关闭 MCP，恢复手动编辑").clicked() {
                        self.set_mcp(false, ctx);
                    }
                } else {
                    ui.small("编辑草稿 / 预览效果 / 导出文件或应用到 Windows");
                }
            });
        if let Some(server) = &mut self.mcp {
            server.update(&mut self.editor);
        }
        egui::TopBottomPanel::bottom("skin-actions")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.set_enabled(!self.close_confirm && self.mcp.is_none());
                if self.editor.actions(ui, &self.path) {
                    if let Ok(config) = core_engine::Config::load(&self.path) {
                        self.config = config;
                        if let Err(e) = crate::app_container::prepare(&self.path, &self.config) {
                            self.error = Some(e);
                        }
                    }
                }
                if !self.editor.message().is_empty() {
                    ui.label(self.editor.message());
                }
                if let Some(error) = self.error.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(ui_style::ERROR, error);
                        if ui.button("知道了").clicked() {
                            self.error = None;
                        }
                    });
                }
            });
        egui::SidePanel::right("skin-preview")
            .resizable(true)
            .default_width(370.0)
            .width_range(300.0..=600.0)
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(234, 239, 245))
                    .inner_margin(20.0),
            )
            .show(ctx, |ui| {
                ui_style::heading(ui, "实时预览", "查看颜色、字号和不同候选状态。");
                egui::ScrollArea::vertical()
                    .id_source("skin-preview-scroll")
                    .show(ui, |ui| self.editor.show_preview(ui));
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ui_style::BACKGROUND)
                    .inner_margin(22.0),
            )
            .show(ctx, |ui| {
                ui.set_enabled(!self.close_confirm);
                egui::ScrollArea::vertical()
                    .id_source("skin-properties")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui_style::card(ui, "", |ui| {
                            if self.mcp.is_some() {
                                self.editor.readonly_ui(ui);
                            } else {
                                self.editor.ui(ui, &mut self.config, &self.path);
                            }
                        });
                    });
            });
        if self.close_confirm {
            egui::Window::new("关闭皮肤编辑器？")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    if self.editor.is_busy() {
                        ui.label("文件操作正在进行，请等待完成后退出。");
                    } else {
                        ui.label("尚未导出的草稿会丢失；已经应用的皮肤不受影响。");
                    }
                    ui.horizontal(|ui| {
                        if ui.button("返回编辑").clicked() {
                            self.close_confirm = false;
                        }
                        if ui
                            .add_enabled(
                                !self.editor.is_busy(),
                                egui::Button::new("放弃草稿并退出"),
                            )
                            .clicked()
                        {
                            self.allow_close = true;
                        }
                    });
                });
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.editor.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// Native preview cached by appearance, skin selection and package metadata.
pub fn active_preview(ui: &mut egui::Ui, config: &core_engine::Config, path: &Path) {
    let key = format!("{:?}-{:?}", config.appearance, config.skins.windows);
    let id = ui.make_persistent_id("active-skin-preview");
    let cached = ui
        .ctx()
        .data_mut(|d| d.get_temp::<(String, Result<egui::TextureHandle, String>)>(id));
    let result = if let Some((old, result)) = cached.filter(|(old, _)| *old == key) {
        let _ = old;
        result
    } else {
        let result = (|| -> Result<egui::TextureHandle, String> {
            let dark = matches!(
                config.skins.windows.mode,
                core_engine::skin::preferences::Mode::Dark
            );
            let skin = core_engine::skin::runtime::load(path, config, "windows", dark, false)
                .map_err(|e| e.to_string())?;
            let mut theme = text_render::candidate_data::ThemeSnapshot::from_parameters(
                skin.parameters.clone(),
            );
            if skin.legacy {
                theme.parameters = None;
            }
            if let Some(package) = &skin.package {
                theme.images = std::sync::Arc::new(
                    text_render::skin_images::decode(package, &skin.images)
                        .map_err(|e| e.to_string())?,
                );
            }
            let data = text_render::candidate_data::CandidateData {
                visible: true,
                spelling: "wq".into(),
                theme,
                items: ["你好", "输入", "五笔"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, text)| text_render::candidate_data::CandidateItem {
                        label: format!("{}. ", i + 1),
                        text: text.into(),
                        hint: String::new(),
                        disabled: false,
                        expression_kind: None,
                    })
                    .collect(),
                ..Default::default()
            };
            let mut renderer = text_render::TextSystem::new().map_err(|e| e.to_string())?;
            let bitmap = text_render::preview::candidate(&mut renderer, &data, 600.0)
                .map_err(|e| e.to_string())?;
            let pixels = bitmap
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| egui::Color32::from_rgba_premultiplied(p[2], p[1], p[0], p[3]))
                .collect();
            Ok(ui.ctx().load_texture(
                "active-candidate",
                egui::ColorImage {
                    size: [bitmap.width as usize, bitmap.height as usize],
                    pixels,
                },
                egui::TextureOptions::LINEAR,
            ))
        })();
        ui.ctx()
            .data_mut(|d| d.insert_temp(id, (key, result.clone())));
        result
    };
    match result {
        Ok(texture) => {
            egui::ScrollArea::horizontal()
                .show(ui, |ui| ui.image((texture.id(), texture.size_vec2())));
        }
        Err(e) => {
            ui.colored_label(ui_style::ERROR, e);
        }
    }
    ui.small("96 DPI 基准预览；跟随系统模式在此展示浅色效果。");
}
