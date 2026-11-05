//! Background download ownership shared by desktop settings and JNI.
use crate::download::{self, Progress, Source};
use core_engine::config::ModelSelection;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[derive(Debug, Clone, Default, Serialize)]
pub struct DownloadStatus {
    pub phase: String,
    pub model: Option<ModelSelection>,
    pub progress: Option<Progress>,
    pub error: Option<String>,
}
struct Task {
    config: PathBuf,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    status: Arc<Mutex<DownloadStatus>>,
}
#[derive(Default)]
pub struct DownloadManager {
    task: Mutex<Option<Task>>,
}

#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
enum Command {
    Available,
    Status {
        config: PathBuf,
    },
    Cancel {
        config: PathBuf,
    },
    Start {
        config: PathBuf,
        model: ModelSelection,
        source: Source,
    },
}
impl DownloadManager {
    /// Short control messages only; network, hashing and package I/O run on the worker.
    pub fn exchange(&self, bytes: &[u8]) -> Vec<u8> {
        let result = (|| -> Result<serde_json::Value, String> {
            if bytes.len() > 65536 {
                return Err("下载请求过大".into());
            }
            let command: Command =
                serde_json::from_slice(bytes).map_err(|_| "下载请求或源标识无效")?;
            if matches!(command, Command::Available) {
                return serde_json::to_value(download::available_models())
                    .map_err(|e| e.to_string());
            }
            let config = match &command {
                Command::Start { config, .. }
                | Command::Cancel { config }
                | Command::Status { config } => config,
                Command::Available => return Err("无效下载请求".into()),
            };
            if !config.is_absolute() {
                return Err("配置路径必须为绝对路径".into());
            }
            let mut slot = self.task.lock().map_err(|_| "下载管理器不可用")?;
            match &command {
                Command::Start { model, source, .. } => {
                    if slot
                        .as_ref()
                        .is_some_and(|t| t.running.load(Ordering::Acquire))
                    {
                        return Err("已有下载任务，请先取消或等待完成".into());
                    }
                    let selected = download::available_models()
                        .into_iter()
                        .find(|m| m.manifest.id == model.id && m.manifest.version == model.version)
                        .ok_or("未适配此模型版本，请使用本地模型包导入")?;
                    let status = Arc::new(Mutex::new(DownloadStatus {
                        phase: "starting".into(),
                        model: Some(model.clone()),
                        ..Default::default()
                    }));
                    let cancel = Arc::new(AtomicBool::new(false));
                    let running = Arc::new(AtomicBool::new(true));
                    let task = Task {
                        config: config.clone(),
                        cancel: Arc::clone(&cancel),
                        running: Arc::clone(&running),
                        status: Arc::clone(&status),
                    };
                    let root = core_engine::config_path::model_store_path(config);
                    let source = *source;
                    std::thread::Builder::new()
                        .name("model-download".into())
                        .spawn(move || {
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    download::download(&selected, source, &root, &cancel, |p| {
                                        if let Ok(mut state) = status.lock() {
                                            state.phase = p.phase.clone();
                                            state.progress = Some(p);
                                        }
                                    })
                                }));
                            if let Ok(mut state) = status.lock() {
                                match result {
                                    Ok(Ok(_)) => state.phase = "ready".into(),
                                    Ok(Err(error)) => {
                                        state.phase = if matches!(
                                            error,
                                            download::DownloadError::Cancelled
                                        ) {
                                            "cancelled"
                                        } else {
                                            "failed"
                                        }
                                        .into();
                                        state.error = Some(error.to_string());
                                    }
                                    Err(_) => {
                                        state.phase = "failed".into();
                                        state.error =
                                            Some("下载线程异常，部分文件保留，请重试".into());
                                    }
                                }
                            }
                            running.store(false, Ordering::Release);
                        })
                        .map_err(|e| format!("无法启动下载：{e}"))?;
                    *slot = Some(task);
                }
                Command::Cancel { .. } => {
                    if let Some(task) = slot
                        .as_ref()
                        .filter(|t| t.config == *config && t.running.load(Ordering::Acquire))
                    {
                        task.cancel.store(true, Ordering::Relaxed);
                        if let Ok(mut state) = task.status.lock() {
                            state.phase = "cancelling".into();
                        }
                    }
                }
                _ => {}
            }
            let state = match slot.as_ref().filter(|t| t.config == *config) {
                Some(task) => task.status.lock().map_err(|_| "下载状态不可用")?.clone(),
                None => DownloadStatus {
                    phase: "idle".into(),
                    ..Default::default()
                },
            };
            serde_json::to_value(state).map_err(|e| e.to_string())
        })();
        serde_json::to_vec(&match result {
            Ok(value) => serde_json::json!({"ok":true,"value":value}),
            Err(error) => serde_json::json!({"ok":false,"error":error}),
        })
        .unwrap_or_default()
    }
}
impl Drop for DownloadManager {
    fn drop(&mut self) {
        if let Ok(slot) = self.task.lock() {
            if let Some(task) = slot.as_ref() {
                task.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn management_rejects_unreviewed_versions_and_never_touches_saved_configuration() {
        let manager = DownloadManager::default();
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "[prediction]\nenabled=false\n").unwrap();
        let before = std::fs::read(&config).unwrap();
        let reply = manager.exchange(&serde_json::to_vec(&serde_json::json!({"op":"Start","config":config,"model":{"id":"unreviewed","version":"v1"},"source":"hugging_face"})).unwrap());
        let reply: serde_json::Value = serde_json::from_slice(&reply).unwrap();
        assert_eq!(reply["ok"], false);
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert!(!core_engine::config_path::model_store_path(&config).exists());
        let reply: serde_json::Value = serde_json::from_slice(
            &manager.exchange(br#"{"op":"Status","config":"relative.toml"}"#),
        )
        .unwrap();
        assert_eq!(reply["ok"], false);
    }
}
