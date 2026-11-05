//! Package operations and hashing stay off the egui thread.
use core_engine::config::ModelSelection;
use model_input::{catalog::Catalog, package::Manifest};
use std::{path::PathBuf, sync::mpsc};

/// Hand the saved path to an independent process before allowing settings to
/// exit. Loading and IPC retries belong to that process, never this UI thread.
pub fn prepare_saved_model(path: &std::path::Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        if let Err(error) = std::process::Command::new(exe.with_file_name("model-worker.exe"))
            .args([std::ffi::OsStr::new("--config"), path.as_os_str()])
            .creation_flags(0x08000000)
            .spawn()
        {
            log::warn!("无法启动已保存配置的模型服务: {error}");
        }
    }
    #[cfg(not(windows))]
    let _ = path;
}

#[derive(Default)]
pub struct ModelCatalog {
    packages: Vec<Manifest>,
    worker: Option<mpsc::Receiver<Result<Vec<Manifest>, String>>>,
    message: String,
    initialized: bool,
    status_worker: Option<mpsc::Receiver<String>>,
    status: String,
}
enum Operation {
    List,
    Import,
    Remove(ModelSelection),
}
impl ModelCatalog {
    fn refresh_status(&mut self, ctx: eframe::egui::Context) {
        if self.status_worker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.status_worker = Some(rx);
        std::thread::spawn(move || {
            #[cfg(windows)]
            let status =
                model_ipc::exchange(br#"{"op":"Status"}"#, std::time::Duration::from_millis(300))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .map(|v| {
                        if v["ok"] != true {
                            "尚未配置模型工作线程".into()
                        } else {
                            match v["value"].as_str() {
                                Some("Loading") => "正在加载和预热".into(),
                                Some("Ready") => "模型已就绪".into(),
                                Some("Paused") => "已暂停，释放资源".into(),
                                Some("Idle") => "空闲，尚未加载".into(),
                                _ => format!("运行失败：{}", v["value"]),
                            }
                        }
                    })
                    .unwrap_or_else(|| "模型服务未运行；输入时按需启动".into());
            #[cfg(not(windows))]
            let status = String::from("此平台不支持 Windows 模型服务");
            let _ = tx.send(status);
            ctx.request_repaint();
        });
    }
    fn start(&mut self, path: PathBuf, operation: Operation, ctx: eframe::egui::Context) {
        if self.worker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.worker = Some(rx);
        self.message = "正在校验模型包…".into();
        std::thread::spawn(move || {
            let catalog = Catalog::for_config(&path);
            let result = (|| {
                match operation {
                    Operation::List => {}
                    Operation::Import => {
                        if let Some(source) = rfd::FileDialog::new()
                            .set_title("选择包含 manifest.json 的模型包目录")
                            .pick_folder()
                        {
                            catalog.import(&source).map_err(|e| e.to_string())?;
                        }
                    }
                    Operation::Remove(selection) => catalog
                        .remove(&selection, &path)
                        .map_err(|e| e.to_string())?,
                }
                catalog.list().map_err(|e| e.to_string())
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub fn show(
        &mut self,
        ui: &mut eframe::egui::Ui,
        path: &std::path::Path,
        config: &mut core_engine::Config,
    ) -> bool {
        if !self.initialized {
            self.initialized = true;
            self.start(path.into(), Operation::List, ui.ctx().clone());
        }
        if let Some(result) = self.worker.as_ref().and_then(|w| w.try_recv().ok()) {
            self.worker = None;
            match result {
                Ok(packages) => {
                    self.packages = packages;
                    self.message = "校验完成；选择后保存设置".into();
                }
                Err(error) => self.message = format!("模型包操作失败：{error}"),
            }
        }
        let before = config.prediction.clone();
        if let Some(status) = self.status_worker.as_ref().and_then(|w| w.try_recv().ok()) {
            self.status = status;
            self.status_worker = None;
        }
        let backend_available = std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent().map(|p| {
                    p.join("model-worker.exe").is_file() && p.join("onnxruntime.dll").is_file()
                })
            })
            .unwrap_or(false);
        ui.collapsing("预测模型与资源", |ui| {
            ui.small("模型完全在本地运行。基础词典预测无需模型；只支持公开规范中的模型包。");
            if !backend_available {ui.label("本安装目录未包含完整模型服务；请使用模型版安装包。词典预测仍可使用。");}
            if ui.add_enabled(self.status_worker.is_none(),eframe::egui::Button::new("刷新运行状态")).clicked() {self.refresh_status(ui.ctx().clone());}
            ui.label(&self.status);
            ui.add_enabled_ui(self.worker.is_none(), |ui| {
                eframe::egui::ComboBox::from_id_source("prediction-model")
                    .selected_text(config.prediction.model.as_ref().map(|m| format!("{} @ {}",m.id,m.version)).unwrap_or_else(|| "仅词典预测".into()))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut config.prediction.model, None, "仅词典预测");
                        for package in self.packages.iter().filter(|p| backend_available && (p.prediction_policy.is_some() || p.generation_policy.is_some()) && p.capabilities.tasks.contains(&core_engine::assistance::Task::Predict)) {
                            ui.selectable_value(&mut config.prediction.model,
                                Some(ModelSelection {id:package.id.clone(),version:package.version.clone()}),
                                format!("{} @ {}",package.id,package.version));
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("导入模型包目录").clicked() { self.start(path.into(),Operation::Import,ui.ctx().clone()); }
                    if ui.button("刷新列表").clicked() { self.start(path.into(),Operation::List,ui.ctx().clone()); }
                });
                let mut remove = None;
                for p in &self.packages {
                    ui.group(|ui| {
                        ui.label(format!("{} @ {} · {:?} · {:.1} MiB",p.id,p.version,p.backend,p.files.iter().map(|f|f.bytes).sum::<u64>() as f64 / 1048576.0));
                        ui.label(format!("许可：{}",p.license)); ui.label(format!("来源：{}",p.source));
                        ui.label(if p.prediction_policy.is_some() || p.generation_policy.is_some() { "已提供预测门槛；不代表效果已通过验收。" } else { "缺少预测门槛：可保留用于离线评测，暂不能用于字词预测。" });
                        ui.label(format!("能力：{:?}；前文 {} 字；最多 {} 候选",p.capabilities.tasks,p.capabilities.max_context_chars,p.capabilities.max_candidates));
                        let selection=ModelSelection{id:p.id.clone(),version:p.version.clone()};
                        if ui.add_enabled(config.prediction.model.as_ref()!=Some(&selection),eframe::egui::Button::new("移除未选模型")).clicked() { remove=Some(selection); }
                    });
                }
                if let Some(selection)=remove { self.start(path.into(),Operation::Remove(selection),ui.ctx().clone()); }
                eframe::egui::ComboBox::from_id_source("model-loading")
                    .selected_text(if config.prediction.loading == core_engine::config::ModelLoading::PreloadKeep { "提前加载并保持" } else { "按需加载" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut config.prediction.loading, core_engine::config::ModelLoading::OnDemand, "按需加载");
                        ui.selectable_value(&mut config.prediction.loading, core_engine::config::ModelLoading::PreloadKeep, "提前加载并保持");
                    });
                ui.add(eframe::egui::Slider::new(&mut config.prediction.wait_ms,50..=1000).text("最长等待（毫秒）"));
                ui.add(eframe::egui::Slider::new(&mut config.prediction.threads,1..=4).text("CPU 线程"));
                ui.add(eframe::egui::Slider::new(&mut config.prediction.max_candidates,1..=64).text("预测候选上限"));
                ui.add(eframe::egui::Slider::new(&mut config.prediction.idle_seconds,10..=600).text("空闲卸载（秒）"));
            });
            ui.small(&self.message);
            ui.small("保存配置不代表模型已加载；模型不可用或超时隐藏本轮预测。尚无通过质量验收的推荐模型。");
        });
        before != config.prediction
    }
}
