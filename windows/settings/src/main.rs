//! # settings
//!
//! MyWubi 配置程序入口。

#![cfg_attr(windows, windows_subsystem = "windows")]

use settings::{app::SettingsApp, config_path, log as log_mod, state::AppState};

fn main() {
    if let Some(code) = settings::automation::run_cli(settings::automation::Application::Settings) {
        std::process::exit(code);
    }
    let personal_args: Vec<_> = std::env::args_os().skip(1).collect();
    if personal_args.len() == 2 && personal_args[0] == "--personal-worker" {
        settings::personal_panel::run_background(std::path::Path::new(&personal_args[1]));
        return;
    }
    if std::env::args().any(|arg| arg == "--component-catalog") {
        use std::io::Write;
        if let Ok(catalog) = runtime_components::catalog() {
            if let Ok(bytes) = serde_json::to_vec(&catalog) {
                let _ = std::io::stdout().write_all(&bytes);
            }
        }
        return;
    }
    // VelopackApp 必须最先运行：处理安装/更新/卸载钩子参数时可能直接退出进程。
    // 正常启动时本调用直接返回，继续执行后续逻辑。
    let isolated = personal_args.iter().any(|arg| arg == "--config");
    if !isolated {
        settings::vpk::init_velopack();
    }

    log_mod::init();

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 2 && args[0] == "--prepare-model" {
        if let Err(error) = settings::model_catalog::run_saved_model(std::path::Path::new(&args[1]))
        {
            log::warn!("模型准备失败：{error}");
        }
        return;
    }

    let tip_access_error = if isolated {
        None
    } else {
        settings::vpk::prepare_tip_access().err()
    };
    if let Some(error) = &tip_access_error {
        log::warn!("{error}");
    }

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

    let capture = match settings::ui_capture::directory(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let explicit = args
        .windows(2)
        .find(|a| a[0] == "--config")
        .map(|a| std::path::PathBuf::from(&a[1]));
    let resolved = explicit
        .map(|p| Ok((p, false, None)))
        .unwrap_or_else(config_path::resolve_config_path);
    let (config_path, portable, fallback_msg) = match resolved {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[ERR] 无法定位配置文件路径: {e}");
            std::process::exit(1);
        }
    };
    log::info!("配置文件路径: {}", config_path.display());

    let mut state = if isolated {
        AppState::load_isolated(config_path)
    } else {
        AppState::load(config_path, portable)
    };
    if let Some(page) = args.windows(2).find(|a| a[0] == "--page") {
        match settings::automation::panel(&page[1].to_string_lossy()) {
            Ok(panel) => state.active_panel = panel,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        }
    }
    if let Some(msg) = fallback_msg {
        state.status_msg = Some(msg);
    }
    if let Some(error) = tip_access_error {
        state.status_msg = Some(format!("[!] {error}"));
    }

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("MyWubi 设置")
        .with_inner_size([1080.0, 780.0])
        .with_min_inner_size([820.0, 620.0]);
    match eframe::icon_data::from_png_bytes(include_bytes!(
        "../../../assets/branding/mywubi-256.png"
    )) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(error) => log::warn!("无法加载应用图标: {error}"),
    }
    let opts = eframe::NativeOptions {
        viewport,
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
        Box::new(|cc| {
            let app = SettingsApp::new(cc, state);
            if let Some(directory) = capture {
                Box::new(settings::ui_capture::CaptureApp::new(app, directory))
            } else {
                Box::new(app)
            }
        }),
    ) {
        eprintln!("[ERR] 启动失败: {e}");
        std::process::exit(2);
    }
}
