//! 关于面板。

use crate::state::AppState;
use eframe::egui::Ui;

pub fn show(ui: &mut Ui, state: &AppState) {
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
}