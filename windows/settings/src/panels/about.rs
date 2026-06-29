//! 关于面板：版本信息 + Velopack 按需更新交互。

use crate::state::{AppState, Panel};
use crate::vpk::{UpdateState, RELEASES_PAGE_URL};
use eframe::egui::{self, Ui};
use tip_manager::TipStatus;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("关于 MyWubi");
    ui.separator();

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
    ui.label("内嵌字体: Noto Sans SC 子集");
    ui.label("GUI 框架: egui / eframe");
    ui.label("依赖致谢: rfd / dirs / simplelog / windows-rs / velopack");

    ui.separator();
    ui.heading("软件更新");
    show_update_ui(ui, state);
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
            ui.colored_label(egui::Color32::GREEN, "✅ 当前已是最新版本。");
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
        UpdateState::Available { version, notes, portable, asset } => {
            ui.colored_label(egui::Color32::from_rgb(33, 150, 243), format!("🆕 发现新版本: {version}"));
            if !notes.trim().is_empty() {
                ui.collapsing("更新说明", |ui| {
                    ui.label(notes);
                });
            }
            if *portable {
                ui.label("当前为便携版，采用被动更新策略。");
                if ui.button("前往下载页").clicked() {
                    crate::vpk::open_releases_page();
                }
            } else {
                // 输入法 DLL 会被 TSF 宿主进程加载锁定，更新前必须先卸载 TIP 释放文件。
                let tip_installed = matches!(state.tip_status, TipStatus::InstalledEnabled | TipStatus::InstalledDisabled);
                if tip_installed {
                    ui.add_space(4.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 152, 0),
                        "⚠️ 检测到输入法已安装。更新前需先卸载输入法以释放 im_engine.dll，否则更新会失败。",
                    );
                    ui.label("请前往「输入法管理」面板点击「卸载」，完成后回到此处继续更新。");
                    if ui.button("前往「输入法管理」").clicked() {
                        state.active_panel = Panel::TipManager;
                    }
                } else {
                    if ui.button("下载并安装").clicked() {
                        state.update_worker = Some(crate::vpk::start_download(asset.clone()));
                        state.update_state = UpdateState::Downloading { progress: 0 };
                    }
                }
            }
        }
        UpdateState::Downloading { progress } => {
            ui.label(format!("正在下载更新… {progress}%"));
            ui.add(egui::ProgressBar::new(*progress as f32 / 100.0).animate(true));
        }
        UpdateState::Ready { asset } => {
            ui.colored_label(egui::Color32::GREEN, "✅ 更新已下载完成。");
            // 应用更新前再次确认 TIP 已卸载（用户可能在下载期间重新安装了输入法）。
            let tip_installed = matches!(state.tip_status, TipStatus::InstalledEnabled | TipStatus::InstalledDisabled);
            if tip_installed {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 152, 0),
                    "⚠️ 输入法仍处于安装状态，请先前往「输入法管理」卸载后再应用更新。",
                );
                if ui.button("前往「输入法管理」").clicked() {
                    state.active_panel = Panel::TipManager;
                }
            } else {
                if ui.button("立即重启以应用更新").clicked() {
                    crate::vpk::apply_and_restart(asset.clone());
                }
                ui.label("点击后将退出程序并自动安装更新，完成后重新启动。");
                ui.label("更新完成后，请前往「输入法管理」面板重新安装输入法。");
            }
        }
        UpdateState::Error(msg) => {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("❌ {msg}"));
            if ui.button("重试").clicked() {
                state.update_worker = Some(crate::vpk::start_check());
                state.update_state = UpdateState::Checking;
            }
        }
    }
}