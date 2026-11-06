//! Download controls use a separate worker and never mutate prediction configuration.
use eframe::egui;
use model_input::{
    download::{self, Source},
    download_manager::DownloadManager,
    package::Manifest,
    service::BackendAvailability,
};
use std::path::Path;

pub struct ModelDownloads {
    manager: DownloadManager,
    source: Source,
    models: Vec<download::DownloadModel>,
    previous_phase: String,
    message: String,
}
impl Default for ModelDownloads {
    fn default() -> Self {
        Self {
            manager: DownloadManager::default(),
            source: Source::HfMirror,
            models: download::available_models(),
            previous_phase: String::new(),
            message: String::new(),
        }
    }
}
impl ModelDownloads {
    fn command(&mut self, value: serde_json::Value) -> serde_json::Value {
        let reply: serde_json::Value =
            serde_json::from_slice(&self.manager.exchange(value.to_string().as_bytes()))
                .unwrap_or_default();
        if reply["ok"] != true {
            self.message = reply["error"].as_str().unwrap_or("下载管理器不可用").into();
        }
        reply["value"].clone()
    }
    /// Returns true once a completed download needs a new installed-package scan.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        config: &Path,
        installed: &[Manifest],
        backends: BackendAvailability,
    ) -> bool {
        let status = self.command(serde_json::json!({"op":"Status","config":config}));
        let phase = status["phase"].as_str().unwrap_or("idle");
        let complete = phase == "ready" && self.previous_phase != "ready";
        self.previous_phase = phase.into();
        let running = matches!(
            phase,
            "starting" | "downloading" | "verifying" | "cancelling"
        );
        ui.separator();
        ui.heading("下载模型（实验性）");
        ui.small("仅下载所选模型；完成后仍需选择、保存并主动开启预测。未完成下载可重试续传。");
        ui.add_enabled_ui(!running, |ui| {
            egui::ComboBox::from_id_source("model-download-source")
                .selected_text(match self.source {
                    Source::HuggingFace => "Hugging Face",
                    Source::HfMirror => "HF 镜像",
                    Source::ModelScope => "ModelScope",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.source, Source::HuggingFace, "Hugging Face");
                    ui.selectable_value(&mut self.source, Source::HfMirror, "HF 镜像");
                    ui.selectable_value(&mut self.source, Source::ModelScope, "ModelScope");
                });
        });
        let mut start = None;
        for model in &self.models {
            ui.group(|ui| {
                let p = &model.manifest;
                ui.strong(&model.title);
                ui.label(format!(
                    "下载 {:.1} MiB · 预估运行内存 {:.0} MiB · {:?}",
                    p.files.iter().map(|f| f.bytes).sum::<u64>() as f64 / 1048576.0,
                    model.memory_bytes as f64 / 1048576.0,
                    p.backend
                ));
                ui.label(format!(
                    "能力：{} · 许可：{}",
                    if p.capabilities.proposes_words {
                        "生成短字词"
                    } else {
                        "候选评分"
                    },
                    p.license
                ));
                ui.small(&model.test_status);
                ui.hyperlink_to("模型来源与许可", &p.source);
                let exists = installed
                    .iter()
                    .any(|i| i.id == p.id && i.version == p.version);
                let mapping = model.validate_source(self.source);
                if let Err(error) = &mapping {
                    ui.label(error.to_string());
                }
                if !backends.supports(p.backend) {
                    ui.label("缺少此模型运行组件；请先按构建说明准备实验性模型版。");
                }
                let label = if exists {
                    "已校验，可在模型选择中使用"
                } else if phase == "failed" || phase == "cancelled" {
                    "重试 / 续传下载"
                } else {
                    "下载模型"
                };
                if ui
                    .add_enabled(
                        !running && !exists && mapping.is_ok() && backends.supports(p.backend),
                        egui::Button::new(label),
                    )
                    .clicked()
                {
                    start = Some(core_engine::config::ModelSelection {
                        id: p.id.clone(),
                        version: p.version.clone(),
                    });
                }
            });
        }
        if let Some(model) = start {
            self.message.clear();
            self.command(serde_json::json!({"op":"Start","config":config,"model":model,"source":self.source}));
        }
        ui.label(match phase {
            "starting" => "正在准备下载",
            "downloading" => "正在下载",
            "verifying" => "下载完成，正在校验并安装",
            "ready" => "校验通过，可选择；总开关未改变",
            "failed" => "下载失败，可重试或换源续传",
            "cancelled" => "已取消，部分文件保留",
            "cancelling" => "正在取消，等待当前网络读取结束",
            _ => "尚未开始下载",
        });
        if let Some(p) = status["progress"].as_object() {
            let completed = p.get("completed").and_then(|n| n.as_u64()).unwrap_or(0);
            let total = p.get("total").and_then(|n| n.as_u64()).unwrap_or(1).max(1);
            ui.add(
                egui::ProgressBar::new(completed as f32 / total as f32).text(format!(
                    "{:.1} / {:.1} MiB",
                    completed as f64 / 1048576.0,
                    total as f64 / 1048576.0
                )),
            );
        }
        if running
            && ui
                .add_enabled(phase != "cancelling", egui::Button::new("取消下载"))
                .clicked()
        {
            self.command(serde_json::json!({"op":"Cancel","config":config}));
        }
        if let Some(error) = status["error"].as_str() {
            ui.label(error);
        }
        ui.small(&self.message);
        if running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }
        complete
    }
}
