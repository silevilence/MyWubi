//! Trial inference stays in a disposable worker process, never the settings process.
use eframe::egui;
use model_input::{package::Manifest, trial::Request};
use std::{path::Path, sync::mpsc};

pub struct ModelTrial {
    guide: bool,
    sample: String,
    selected: Option<String>,
    result: String,
    worker: Option<mpsc::Receiver<String>>,
}
impl Default for ModelTrial {
    fn default() -> Self {
        Self {
            guide: false,
            sample: "北京".into(),
            selected: None,
            result: String::new(),
            worker: None,
        }
    }
}
impl ModelTrial {
    pub fn show(&mut self, ui: &mut egui::Ui, config: &Path, packages: &[Manifest]) {
        if ui.button("自定义模型指南与试运行").clicked() {
            self.guide = !self.guide;
        }
        if !self.guide {
            return;
        }
        ui.collapsing("接入指南（离线可读）", |ui| {
            ui.label(model_input::trial::GUIDE);
        });
        if let Some(value) = self.worker.as_ref().and_then(|r| r.try_recv().ok()) {
            self.worker = None;
            self.result = value;
        }
        ui.label("显式测试前文；不会读取编辑器或更改活动模型。最多 256 字。");
        ui.add_enabled_ui(self.worker.is_none(), |ui| {
            let previous = (self.selected.clone(), self.sample.clone());
            egui::ComboBox::from_id_source("trial-package")
                .selected_text(self.selected.as_deref().unwrap_or("选择试运行模型"))
                .show_ui(ui, |ui| {
                    for p in packages {
                        let name = format!("{}--{}", p.id, p.version);
                        ui.selectable_value(&mut self.selected, Some(name.clone()), name);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.sample)
                    .char_limit(256)
                    .hint_text("例如：北京"),
            );
            if previous != (self.selected.clone(), self.sample.clone()) {
                self.result.clear();
            }
            if ui
                .add_enabled(
                    self.selected.is_some() && !self.sample.trim().is_empty(),
                    egui::Button::new("试运行（1000 ms，2 线程）"),
                )
                .clicked()
            {
                if let Some(name) = &self.selected {
                    let request = Request {
                        package: core_engine::config_path::model_store_path(config).join(name),
                        prefix: self.sample.clone(),
                        wait_ms: 1000,
                        threads: 2,
                    };
                    let (tx, rx) = mpsc::channel();
                    self.worker = Some(rx);
                    self.result = "正在校验、加载与试运行…".into();
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(run_process(request).unwrap_or_else(|e| e));
                        ctx.request_repaint();
                    });
                }
            }
        });
        if self.worker.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(500));
        }
        ui.label(&self.result);
        ui.small("试运行成功不代表质量达标，不会自动选择、开启预测或上屏。");
    }
}

fn run_process(request: Request) -> Result<String, String> {
    use std::{
        io::Write,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let exe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("model-worker.exe");
    let mut command = Command::new(exe);
    command
        .arg("--trial")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "试运行组件缺失；请安装模型版发行包")?;
    let send = (|| {
        let mut stdin = child.stdin.take().ok_or("无法写入测试请求")?;
        stdin
            .write_all(&serde_json::to_vec(&request).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    })();
    if let Err(error) = send {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(90) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("试运行进程未在 90 秒内完成，已停止；当前输入未改变".into());
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let reply: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "试运行组件版本不兼容或进程异常退出")?;
    if reply["ok"] != true {
        return Err(format!(
            "{}（总耗时 {} ms）",
            reply["error"].as_str().unwrap_or("试运行失败"),
            reply["elapsed_ms"]
        ));
    }
    let value = &reply["value"];
    Ok(format!(
        "候选：{}\n加载与预热 {} ms；推理 {} ms\n{}",
        value["words"],
        value["load_ms"],
        value["predict_ms"],
        value["notice"].as_str().unwrap_or_default()
    ))
}
