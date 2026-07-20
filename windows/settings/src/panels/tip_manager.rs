//! 输入法管理面板：TIP 状态显示与安装/启用/禁用/卸载操作。

use crate::state::AppState;
use eframe::egui::{self, Ui};
use tip_manager::TipStatus;

/// 渲染「输入法管理」面板。
pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("输入法管理");
    ui.add_space(8.0);

    // TIP 注册/卸载需要管理员权限。非管理员运行时仅显示状态与提升入口，
    // 不直接调用 tip_manager（避免 COM 调用静默失败或权限错误）。
    let is_admin = crate::elevation::is_admin();
    if !is_admin {
        ui.colored_label(
            egui::Color32::from_rgb(255, 152, 0),
            "⚠️ 当前未以管理员身份运行，无法安装或卸载输入法。",
        );
        ui.add_space(4.0);
        ui.label("输入法的注册需要写入系统注册表，请以管理员身份重新运行设置程序。");
        ui.add_space(8.0);
        if ui.button("以管理员身份重启").clicked() {
            if crate::elevation::relaunch_elevated() {
                // 发起重启后退出当前非提升实例。
                std::process::exit(0);
            } else {
                state.status_msg = Some(
                    "❌ 无法发起管理员重启，请手动右键 settings.exe 选择「以管理员身份运行」。"
                        .into(),
                );
            }
        }
        ui.separator();
        ui.add_space(8.0);
        ui.label("当前状态（只读）:");
        ui.add_space(4.0);
    }

    match state.tip_status {
        TipStatus::NotInstalled => show_not_installed(ui, state, is_admin),
        TipStatus::InstalledEnabled => show_installed_enabled(ui, state, is_admin),
        TipStatus::InstalledDisabled => show_installed_disabled(ui, state, is_admin),
        TipStatus::Unknown => show_unknown(ui, state, is_admin),
    }

    if let Some(ref msg) = state.status_msg {
        if msg.starts_with("❌") {
            ui.add_space(8.0);
            ui.collapsing("查看详情", |ui| {
                ui.label(msg);
            });
        }
    }
}

fn status_badge(ui: &mut Ui, color: egui::Color32, text: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(color, "●");
        ui.label(text);
    });
}

fn show_not_installed(ui: &mut Ui, state: &mut AppState, is_admin: bool) {
    status_badge(ui, egui::Color32::GRAY, "未安装");
    ui.add_space(8.0);
    ui.label("MyWubi 输入法尚未安装到系统中。点击下方按钮完成安装，即可在语言设置中添加此输入法。");
    ui.add_space(12.0);

    ui.add_enabled_ui(is_admin, |ui| {
        if ui.button("安装输入法").clicked() {
            let dll_path = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("im_engine.dll")))
                .and_then(|p| p.to_str().map(String::from));

            match dll_path {
                Some(path) => match tip_manager::install(&path) {
                    Ok(()) => {
                        state.tip_status = tip_manager::detect_status();
                        state.status_msg =
                            Some("✅ 输入法安装成功！使用 Win+Space 切换至此输入法。".into());
                    }
                    Err(e) => {
                        state.status_msg = Some(format!("❌ 安装失败: {e}"));
                    }
                },
                None => {
                    state.status_msg = Some(
                        "❌ 找不到 im_engine.dll，请确保它与 settings.exe 在同一目录。".into(),
                    );
                }
            }
        }
    });

    ui.add_space(8.0);
    ui.label("安装需要管理员权限。");
}

fn show_installed_enabled(ui: &mut Ui, state: &mut AppState, is_admin: bool) {
    status_badge(ui, egui::Color32::GREEN, "已安装 · 已启用");
    ui.add_space(8.0);
    ui.label("输入法运行正常。使用 Win+Space 切换至此输入法即可开始打字。");
    ui.add_space(8.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let dll = dir.join("im_engine.dll");
                ui.label(format!("DLL: {}", dll.display()));
            }
        }
        ui.label("CLSID: {C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD8}");
    });
    ui.add_space(8.0);

    ui.add_enabled_ui(is_admin, |ui| {
        ui.horizontal(|ui| {
            if ui.button("禁用").clicked() {
                match tip_manager::disable() {
                    Ok(()) => {
                        state.tip_status = tip_manager::detect_status();
                        state.status_msg = Some("✅ 输入法已禁用".into());
                    }
                    Err(e) => {
                        state.status_msg = Some(format!("❌ 禁用失败: {e}"));
                    }
                }
            }
            if state.uninstall_confirm {
                if ui.button("确认卸载").clicked() {
                    state.uninstall_confirm = false;
                    match tip_manager::uninstall() {
                        Ok(()) => {
                            state.tip_status = tip_manager::detect_status();
                            state.status_msg = Some("✅ 输入法已卸载".into());
                        }
                        Err(e) => {
                            state.status_msg = Some(format!("❌ 卸载失败: {e}"));
                        }
                    }
                }
                if ui.button("取消").clicked() {
                    state.uninstall_confirm = false;
                }
            } else {
                if ui.button("卸载").clicked() {
                    state.uninstall_confirm = true;
                    state.status_msg =
                        Some("⚠️ 确定要卸载 MyWubi 输入法吗？请再次点击「确认卸载」".into());
                }
            }
        });
    });
}

fn show_installed_disabled(ui: &mut Ui, state: &mut AppState, is_admin: bool) {
    status_badge(ui, egui::Color32::from_rgb(255, 152, 0), "已安装 · 已禁用");
    ui.add_space(8.0);
    ui.label("输入法已安装但当前被禁用。启用后可在语言设置中选择。");
    ui.add_space(12.0);

    ui.add_enabled_ui(is_admin, |ui| {
        ui.horizontal(|ui| {
            if ui.button("启用").clicked() {
                match tip_manager::enable() {
                    Ok(()) => {
                        state.tip_status = tip_manager::detect_status();
                        state.status_msg = Some("✅ 输入法已启用".into());
                    }
                    Err(e) => {
                        state.status_msg = Some(format!("❌ 启用失败: {e}"));
                    }
                }
            }
            if state.uninstall_confirm {
                if ui.button("确认卸载").clicked() {
                    state.uninstall_confirm = false;
                    match tip_manager::uninstall() {
                        Ok(()) => {
                            state.tip_status = tip_manager::detect_status();
                            state.status_msg = Some("✅ 输入法已卸载".into());
                        }
                        Err(e) => {
                            state.status_msg = Some(format!("❌ 卸载失败: {e}"));
                        }
                    }
                }
                if ui.button("取消").clicked() {
                    state.uninstall_confirm = false;
                }
            } else {
                if ui.button("卸载").clicked() {
                    state.uninstall_confirm = true;
                    state.status_msg =
                        Some("⚠️ 确定要卸载 MyWubi 输入法吗？请再次点击「确认卸载」".into());
                }
            }
        });
    });
}

fn show_unknown(ui: &mut Ui, state: &mut AppState, is_admin: bool) {
    status_badge(ui, egui::Color32::RED, "状态异常");
    ui.add_space(8.0);
    ui.label("检测到不完整的安装状态。建议尝试修复或完全卸载后重新安装。");
    ui.add_space(12.0);

    ui.add_enabled_ui(is_admin, |ui| {
        ui.horizontal(|ui| {
            if ui.button("修复安装").clicked() {
                let dll_path = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("im_engine.dll")))
                    .and_then(|p| p.to_str().map(String::from));

                if let Some(path) = dll_path {
                    match tip_manager::install(&path) {
                        Ok(()) => {
                            state.tip_status = tip_manager::detect_status();
                            state.status_msg = Some("✅ 修复成功".into());
                        }
                        Err(e) => {
                            state.status_msg = Some(format!("❌ 修复失败: {e}"));
                        }
                    }
                }
            }
            if ui.button("完全卸载").clicked() {
                match tip_manager::uninstall() {
                    Ok(()) => {
                        state.tip_status = tip_manager::detect_status();
                        state.status_msg = Some("✅ 已完全卸载".into());
                    }
                    Err(e) => {
                        state.status_msg = Some(format!("❌ 卸载失败: {e}"));
                    }
                }
            }
        });
    });
}
