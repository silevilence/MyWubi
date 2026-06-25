//! # settings
//!
//! MyWubi 配置程序入口。

use settings::{app::SettingsApp, config_path, log as log_mod, state::AppState};

fn main() {
    log_mod::init();
    let (config_path, fallback_msg) = match config_path::resolve_config_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[ERR] 无法定位配置文件路径: {e}");
            std::process::exit(1);
        }
    };
    log::info!("配置文件路径: {}", config_path.display());

    let mut state = AppState::load(config_path);
    if let Some(msg) = fallback_msg {
        state.status_msg = Some(msg);
    }

    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("MyWubi 设置")
            .with_inner_size([900.0, 650.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "MyWubi 设置",
        opts,
        Box::new(|cc| Box::new(SettingsApp::new(cc, state))),
    ) {
        eprintln!("[ERR] 启动失败: {e}");
        std::process::exit(2);
    }
}