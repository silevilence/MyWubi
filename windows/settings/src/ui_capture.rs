//! Bounded native-render capture for visual regression; no desktop clicking is needed.
use eframe::egui;
use std::path::PathBuf;

pub trait CaptureTarget: eframe::App {
    fn capture_cases(&self) -> &'static [&'static str];
    fn prepare_capture(&mut self, case: &str, ctx: &egui::Context);
    fn capture_ready(&self) -> bool {
        true
    }
}

pub struct CaptureApp<T> {
    app: T,
    directory: PathBuf,
    index: usize,
    frames: usize,
    requested: bool,
    started: std::time::Instant,
}

impl<T: CaptureTarget> CaptureApp<T> {
    pub fn new(app: T, directory: PathBuf) -> Self {
        Self {
            app,
            directory,
            index: 0,
            frames: 0,
            requested: false,
            started: std::time::Instant::now(),
        }
    }
}

impl<T: CaptureTarget> eframe::App for CaptureApp<T> {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.started.elapsed().as_secs() > 45 {
            log::error!("UI capture timed out");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let screenshot = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            let file = self
                .directory
                .join(format!("{}.png", self.app.capture_cases()[self.index]));
            let rgba: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Err(e) = image::save_buffer(
                &file,
                &rgba,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            ) {
                log::error!("Cannot save {}: {e}", file.display());
            }
            self.index += 1;
            self.frames = 0;
            self.requested = false;
        }
        if self.index == self.app.capture_cases().len() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.frames == 0 {
            self.app
                .prepare_capture(self.app.capture_cases()[self.index], ctx);
        }
        self.app.update(ctx, frame);
        self.frames += 1;
        if self.frames >= 6 && !self.requested && self.app.capture_ready() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            self.requested = true;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(70));
    }
}

impl CaptureTarget for crate::app::SettingsApp {
    fn capture_ready(&self) -> bool {
        !self.state.expressions.is_loading()
    }

    fn capture_cases(&self) -> &'static [&'static str] {
        &[
            "basic",
            "appearance",
            "dictionary",
            "special_input",
            "intelligence",
            "tip_manager",
            "about",
            "basic-compact",
            "appearance-compact",
        ]
    }
    fn prepare_capture(&mut self, case: &str, ctx: &egui::Context) {
        if case.ends_with("-compact") {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(820.0, 620.0)));
        }
        if let Ok(panel) = crate::automation::panel(case.trim_end_matches("-compact")) {
            self.state.active_panel = panel;
        }
    }
}

impl CaptureTarget for crate::skin_app::SkinApp {
    fn capture_cases(&self) -> &'static [&'static str] {
        &[
            "skin-empty",
            "skin-light",
            "skin-dark",
            "skin-android",
            "skin-keyboard",
            "skin-compact",
        ]
    }
    fn prepare_capture(&mut self, case: &str, ctx: &egui::Context) {
        if case == "skin-compact" {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1000.0, 660.0)));
        }
        self.editor.prepare_capture(case);
    }
}

/// Capture requires an explicit config and a new output directory to protect real sessions.
pub fn directory(args: &[std::ffi::OsString]) -> Result<Option<PathBuf>, String> {
    let Some(pair) = args.windows(2).find(|a| a[0] == "--capture-ui") else {
        return Ok(None);
    };
    if !args.windows(2).any(|a| a[0] == "--config") {
        return Err("--capture-ui 必须同时指定 --config 测试配置".into());
    }
    let path = PathBuf::from(&pair[1]);
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    Ok(Some(path))
}
