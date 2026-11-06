//! Component management keeps file hashing and installation off the egui thread.
use eframe::egui;
use model_input::{
    components::{self, Progress},
    package::Manifest,
};
use runtime_components as runtime;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
};

type Installed = BTreeMap<String, Result<(), String>>;
type ScanResult = Result<(Vec<runtime::Component>, Installed), String>;

#[derive(Default)]
pub struct Components {
    catalog: Vec<runtime::Component>,
    installed: Installed,
    worker: Option<mpsc::Receiver<ScanResult>>,
    progress: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    confirm: Option<(String, bool)>,
    message: String,
    initialized: bool,
}
impl Components {
    fn start(&mut self, config: &Path, operation: Option<(String, bool)>, ctx: egui::Context) {
        if self.worker.is_some() {
            return;
        }
        let path = config.to_path_buf();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Arc::clone(&cancel);
        let progress = Arc::clone(&self.progress);
        if let Ok(mut p) = progress.lock() {
            *p = Progress {
                phase: "preparing".into(),
                ..Default::default()
            };
        }
        let (tx, rx) = mpsc::channel();
        self.worker = Some(rx);
        std::thread::spawn(move || {
            let result = (|| {
                if let Some((key, remove)) = operation {
                    if remove {
                        components::remove(&path, &key)?;
                    } else {
                        components::install(&path, &key, &cancel, |p| {
                            if let Ok(mut state) = progress.lock() {
                                *state = p;
                            }
                            ctx.request_repaint();
                        })?;
                    }
                }
                let mut all = runtime::catalog().map_err(|e| e.to_string())?;
                for installed in
                    runtime::installed(&runtime::store(&path)).map_err(|e| e.to_string())?
                {
                    if !all.iter().any(|c| c.key() == installed.key()) {
                        all.push(installed);
                    }
                }
                let states = all
                    .iter()
                    .map(|c| {
                        (
                            c.key(),
                            runtime::verify(&runtime::store(&path), c)
                                .map(|_| ())
                                .map_err(|e| e.to_string()),
                        )
                    })
                    .collect();
                Ok((all, states))
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    /// True means resource state changed; caller should refresh service capabilities.
    pub fn show(&mut self, ui: &mut egui::Ui, path: &Path, models: &[Manifest]) -> bool {
        if !self.initialized {
            self.initialized = true;
            match runtime::catalog() {
                Ok(all) => self.catalog = all,
                Err(e) => self.message = e.to_string(),
            }
            self.start(path, None, ui.ctx().clone());
        }
        let mut changed = false;
        if let Some(result) = self.worker.as_ref().and_then(|r| r.try_recv().ok()) {
            self.worker = None;
            match result {
                Ok((catalog, installed)) => {
                    self.catalog = catalog;
                    self.installed = installed;
                    self.message = "组件检查完成；预测设置未改变".into();
                    changed = true;
                }
                Err(e) => self.message = e,
            }
        }
        ui.heading("推理组件");
        ui.small("基础输入和词典预测无需组件。GGUF 与 ONNX 共用 CPU 组件；模型文件另行下载。");
        if self.catalog.is_empty() {
            ui.label("当前构建尚未附带已发布组件清单；无法下载组件。");
        }
        let mut confirm = None;
        let trusted = runtime::catalog().unwrap_or_default();
        for c in &self.catalog {
            ui.group(|ui| {
                ui.strong(format!("{} · {}", c.id, c.version));
                if !trusted.iter().any(|known| known.key() == c.key()) {
                    ui.label("旧组件版本；当前应用不会自动启用，可在不被占用时移除。");
                }
                ui.label(format!(
                    "{} · {} · 下载 {:.1} MiB · 安装 {:.1} MiB",
                    c.os,
                    c.architecture,
                    c.archive.bytes as f64 / 1048576.0,
                    c.installed_bytes as f64 / 1048576.0
                ));
                ui.small(format!(
                    "能力：{}；许可：{}",
                    c.capabilities.join(" / "),
                    c.license
                ));
                let installed = self.installed.get(&c.key()).is_some_and(Result::is_ok);
                ui.label(if installed {
                    "已安装，可离线使用"
                } else {
                    "缺推理组件或校验失败"
                });
                if let Some(Err(e)) = self.installed.get(&c.key()) {
                    ui.small(e);
                }
                ui.add_enabled_ui(self.worker.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        if let Some(capability) = c.capabilities.first() {
                            if ui
                                .add_enabled(
                                    !installed
                                        && trusted.iter().any(|known| known.key() == c.key()),
                                    egui::Button::new("准备下载 / 更新"),
                                )
                                .clicked()
                            {
                                confirm = Some((capability.clone(), false));
                            }
                        }
                        if ui
                            .add_enabled(installed, egui::Button::new("移除组件…"))
                            .clicked()
                        {
                            confirm = Some((c.key(), true));
                        }
                    });
                });
            });
        }
        if confirm.is_some() {
            self.confirm = confirm;
        }
        if let Some((key, remove)) = self.confirm.clone() {
            ui.group(|ui| {
                if remove {
                    let affected: Vec<_> = models.iter().filter(|m| self.catalog.iter().any(|c| c.key() == key && c.capabilities.iter().any(|s| s == components::capability(m.backend)))).map(|m| m.id.as_str()).collect();
                    ui.label(format!("移除此组件后，下列模型将不能运行：{}。活动组件和共享依赖会被保护。", if affected.is_empty() { "当前无已安装模型".into() } else { affected.join("、") }));
                } else {
                    match runtime::plan(&self.catalog, &key, env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH) {
                        Ok(plan) => {
                            let pending: Vec<_> = plan.iter().filter(|c| !self.installed.get(&c.key()).is_some_and(Result::is_ok)).collect();
                            ui.label(format!("确认下载 {} 个组件，共 {:.1} MiB，安装占用 {:.1} MiB。来源：MyWubi 官方 GitHub Release。", pending.len(), pending.iter().map(|c| c.archive.bytes).sum::<u64>() as f64 / 1048576.0, pending.iter().map(|c| c.installed_bytes).sum::<u64>() as f64 / 1048576.0));
                        }
                        Err(e) => { ui.label(e.to_string()); }
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button(if remove { "确认移除" } else { "确认下载 / 重试续传" }).clicked() { self.confirm = None; self.start(path, Some((key.clone(), remove)), ui.ctx().clone()); }
                    if ui.button("取消").clicked() { self.confirm = None; }
                });
            });
        }
        if self.worker.is_some() {
            if let Ok(p) = self.progress.lock() {
                ui.label(&p.message);
                ui.add(
                    egui::ProgressBar::new(p.completed as f32 / p.total.max(1) as f32).text(
                        format!(
                            "{:.1} / {:.1} MiB",
                            p.completed as f64 / 1048576.0,
                            p.total as f64 / 1048576.0
                        ),
                    ),
                );
            }
            if ui.button("取消组件下载").clicked() {
                self.cancel.store(true, Ordering::Relaxed);
                self.message = "正在取消；部分文件将保留以便续传".into();
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        } else if ui.button("重新检查组件").clicked() {
            self.start(path, None, ui.ctx().clone());
        }
        ui.label(&self.message);
        changed
    }
}
impl Drop for Components {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
