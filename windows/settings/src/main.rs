//! # settings
//!
//! MyWubi 配置程序入口。

#![cfg_attr(windows, windows_subsystem = "windows")]

use settings::{app::SettingsApp, config_path, log as log_mod, state::AppState};

fn main() {
    // VelopackApp 必须最先运行：处理安装/更新/卸载钩子参数时可能直接退出进程。
    // 正常启动时本调用直接返回，继续执行后续逻辑。
    settings::vpk::init_velopack();

    log_mod::init();

    #[cfg(windows)]
    {
        // COM 初始化（tip_manager 的 ITfInputProcessorProfileMgr 调用需要）。
        // 不再强制管理员权限——非管理员也能查看/修改配置；仅「输入法管理」
        // 面板的 TIP 注册/卸载操作需要管理员，由该面板按需触发提升重启。
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
    }

    let (config_path, portable, fallback_msg) = match config_path::resolve_config_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[ERR] 无法定位配置文件路径: {e}");
            std::process::exit(1);
        }
    };
    log::info!("配置文件路径: {}", config_path.display());

    let mut state = AppState::load(config_path, portable);
    if let Some(msg) = fallback_msg {
        state.status_msg = Some(msg);
    }

    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("MyWubi 设置")
            .with_inner_size([900.0, 650.0])
            .with_min_inner_size([640.0, 480.0]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            supported_backends: wgpu::Backends::DX12,
            ..Default::default()
        },
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
