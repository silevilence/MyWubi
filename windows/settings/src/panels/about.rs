//! 关于面板：版本信息 + Velopack 按需更新交互。

use crate::state::AppState;
use crate::vpk::{UpdateState, RELEASES_PAGE_URL};
use eframe::egui::{self, Ui};

pub fn show(ui: &mut Ui, state: &mut AppState) {
    crate::ui_style::heading(ui, "关于与更新", "版本信息、配置位置与软件更新。");
    crate::ui_style::card(ui, "MyWubi", |ui| {
        let texture_id = egui::Id::new("mywubi_brand_texture");
        let texture = ui
            .ctx()
            .data_mut(|data| data.get_temp::<egui::TextureHandle>(texture_id))
            .or_else(|| {
                let icon = eframe::icon_data::from_png_bytes(include_bytes!(
                    "../../../../assets/branding/mywubi-256.png"
                ))
                .ok()?;
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                );
                let texture = ui
                    .ctx()
                    .load_texture("MyWubi", image, egui::TextureOptions::LINEAR);
                ui.ctx()
                    .data_mut(|data| data.insert_temp(texture_id, texture.clone()));
                Some(texture)
            });
        if let Some(texture) = texture {
            ui.image((texture.id(), egui::vec2(64.0, 64.0)));
        }

        ui.label(format!("版本: {}", env!("CARGO_PKG_VERSION")));
        ui.label("跨平台形码输入法配置程序");
        ui.hyperlink_to("项目仓库", env!("CARGO_PKG_REPOSITORY"));

        ui.separator();
        ui.label("当前配置文件路径:");
        ui.monospace(state.config_path.display().to_string());
        ui.label(if state.portable {
            "（便携模式）"
        } else {
            "（用户模式）"
        });

        ui.separator();
        ui.label("界面字体: 系统字体优先，Noto Sans SC 子集回退");
        ui.label("GUI 框架: egui / eframe");
        ui.label("依赖致谢: rfd / dirs / simplelog / windows-rs / velopack");

        ui.separator();
    });
    crate::ui_style::card(ui, "软件更新", |ui| {
        show_update_ui(ui, state);
    });
}

fn show_update_ui(ui: &mut Ui, state: &mut AppState) {
    // 触发后台 worker 轮询由 app.rs 统一处理；此处仅渲染状态与按钮。
    match &state.update_state {
        UpdateState::Idle => {
            if ui.button("检查更新").clicked() {
                state.update_worker = Some(crate::vpk::start_check());
                state.update_state = UpdateState::Checking;
            }
            ui.label("点击检查是否有新版本。");
        }
        UpdateState::Checking => {
            ui.spinner();
            ui.label("正在检查更新…");
        }
        UpdateState::NoUpdate => {
            ui.colored_label(crate::ui_style::SUCCESS, "✅ 当前已是最新版本。");
            if ui.button("再次检查").clicked() {
                state.update_worker = Some(crate::vpk::start_check());
                state.update_state = UpdateState::Checking;
            }
        }
        UpdateState::NotInstalled => {
            ui.label("当前为非安装版运行（如开发期或绿色解压版），自动更新不可用。");
            ui.hyperlink_to("前往发布页查看新版本", RELEASES_PAGE_URL);
            if ui.button("重新检查").clicked() {
                state.update_worker = Some(crate::vpk::start_check());
                state.update_state = UpdateState::Checking;
            }
        }
        UpdateState::Available {
            version,
            notes,
            portable,
            selected,
        } => {
            ui.colored_label(
                egui::Color32::from_rgb(33, 150, 243),
                format!("🆕 发现新版本: {version}"),
            );
            if !notes.trim().is_empty() {
                ui.collapsing("更新说明", |ui| {
                    ui.label(notes);
                });
            }
            if *portable {
                ui.label("当前为便携版，采用被动更新策略。");
                ui.hyperlink_to("前往下载页", selected.release_page());
            } else if ui.button("下载更新").clicked() {
                state.update_worker = Some(crate::vpk::start_download(selected.clone()));
                state.update_state = UpdateState::Downloading { progress: 0 };
            }
        }
        UpdateState::Downloading { progress } => {
            ui.label(format!("正在下载更新… {progress}%"));
            ui.add(egui::ProgressBar::new(*progress as f32 / 100.0).animate(true));
        }
        UpdateState::Ready { asset } => {
            ui.label(format!(
                "{} 已下载，当前运行 {}。",
                asset.Version,
                env!("CARGO_PKG_VERSION")
            ));
            if ui
                .add_enabled(
                    !state.has_unsaved_changes(),
                    egui::Button::new("立即重启以应用更新"),
                )
                .on_disabled_hover_text("请先保存当前修改")
                .clicked()
            {
                if let Err(error) = crate::vpk::apply_and_restart(asset) {
                    state.status_msg = Some(format!("[ERR] 无法启动更新：{error}"));
                }
            }
            ui.collapsing("上次更新没有完成？", |ui| {
                ui.label("请保存工作，关闭使用过输入法的应用后重试。若安装目录仍被占用，可注销 Windows，再登录后继续安装。");
                ui.label(r"更新日志：%LOCALAPPDATA%\velopack\velopack_MyWubi.log");
            });
        }
        UpdateState::Error(msg) => {
            ui.colored_label(crate::ui_style::ERROR, format!("❌ {msg}"));
            if ui.button("重试").clicked() {
                if let Some(asset) = crate::vpk::pending_update() {
                    state.update_state = UpdateState::Ready { asset };
                } else {
                    state.update_worker = Some(crate::vpk::start_check());
                    state.update_state = UpdateState::Checking;
                }
            }
        }
    }
}
