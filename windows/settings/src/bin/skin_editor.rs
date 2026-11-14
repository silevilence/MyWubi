#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if let Some(code) = settings::automation::run_cli(settings::automation::Application::SkinEditor)
    {
        std::process::exit(code);
    }
    settings::log::init();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let capture = match settings::ui_capture::directory(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let path = args
        .windows(2)
        .find(|a| a[0] == "--config")
        .map(|a| std::path::PathBuf::from(&a[1]));
    let path = match path
        .map(Ok)
        .unwrap_or_else(|| settings::config_path::resolve_config_path().map(|v| v.0))
    {
        Ok(path) => path,
        Err(e) => {
            log::error!("无法定位配置：{e}");
            std::process::exit(1);
        }
    };
    let mut editor = settings::skin_editor::SkinEditor::default();
    if let Some(open) = args.windows(2).find(|a| a[0] == "--open") {
        if let Err(e) = editor.open_file(std::path::Path::new(&open[1])) {
            log::error!("{e}");
            std::process::exit(1);
        }
    }
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("MyWubi 皮肤编辑器")
        .with_inner_size([1180.0, 800.0])
        .with_min_inner_size([1000.0, 660.0]);
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            supported_backends: wgpu::Backends::DX12,
            ..Default::default()
        },
        ..Default::default()
    };
    if let Err(e) = eframe::run_native(
        "MyWubi 皮肤编辑器",
        options,
        Box::new(|cc| {
            let app = settings::skin_app::SkinApp::new(cc, path, editor);
            if let Some(directory) = capture {
                Box::new(settings::ui_capture::CaptureApp::new(app, directory))
            } else {
                Box::new(app)
            }
        }),
    ) {
        log::error!("皮肤编辑器启动失败：{e}");
        std::process::exit(2);
    }
}
