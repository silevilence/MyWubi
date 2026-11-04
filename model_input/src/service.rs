//! Small management facade shared by JNI and the isolated desktop process.
use crate::worker::Worker;
use core_engine::{
    assistance::{RequestKey, ScoreRequest},
    config::PredictionConfig,
};
use std::{path::PathBuf, sync::Mutex};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum Command {
    Features,
    Configure {
        root: PathBuf,
        config: PredictionConfig,
    },
    Submit {
        request: ScoreRequest,
    },
    Poll {
        key: RequestKey,
    },
    Invalidate,
    Pause {
        paused: bool,
    },
    Status,
    Close,
}
#[derive(Default)]
pub struct Endpoint {
    worker: Mutex<Option<(PathBuf, Worker)>>,
}
impl Endpoint {
    pub fn exchange(&self, bytes: &[u8]) -> Vec<u8> {
        let result = (|| -> Result<serde_json::Value, String> {
            if bytes.len() > 64 * 1024 {
                return Err("request exceeds limit".into());
            }
            let command: Command =
                serde_json::from_slice(bytes).map_err(|_| "invalid inference request")?;
            let mut slot = self
                .worker
                .lock()
                .map_err(|_| "inference owner unavailable")?;
            match command {
                Command::Features => Ok(
                    serde_json::json!({"llama":cfg!(feature="llama-cpp-2"),"onnx":cfg!(feature="ort")}),
                ),
                Command::Configure { root, config } => {
                    if !root.is_absolute() {
                        return Err("model store must be absolute".into());
                    }
                    let check = core_engine::Config {
                        prediction: config.clone(),
                        ..Default::default()
                    };
                    check
                        .validate_for(core_engine::config::ValidationContext::Read)
                        .map_err(|_| "invalid inference configuration")?;
                    if slot.is_none() {
                        *slot = Some((
                            root.clone(),
                            Worker::new(root, config)
                                .map_err(|_| "inference worker could not start")?,
                        ));
                    } else if let Some((path, worker)) = slot.as_mut() {
                        *path = root.clone();
                        worker.configure_root(root, config);
                    }
                    Ok(serde_json::Value::Null)
                }
                Command::Close => {
                    // Retain one sleeping thread. Rapid close/reopen or root
                    // switches must not create concurrent uncancellable loaders.
                    if let Some((_, worker)) = slot.as_ref() {
                        worker.pause(true);
                        worker.configure(PredictionConfig::default());
                    }
                    Ok(serde_json::Value::Null)
                }
                command => {
                    let Some((_, worker)) = slot.as_ref() else {
                        return Err("inference worker is not configured".into());
                    };
                    match command {
                        Command::Submit { request } => {
                            worker.submit(request).map_err(|e| e.to_string())?;
                            Ok(serde_json::Value::Null)
                        }
                        Command::Poll { key } => Ok(serde_json::json!(worker.take(key))),
                        Command::Invalidate => {
                            worker.invalidate();
                            Ok(serde_json::Value::Null)
                        }
                        Command::Pause { paused } => {
                            worker.pause(paused);
                            Ok(serde_json::Value::Null)
                        }
                        Command::Status => Ok(serde_json::json!(worker.phase())),
                        _ => unreachable!(),
                    }
                }
            }
        })();
        serde_json::to_vec(&match result {
            Ok(value) => serde_json::json!({"ok":true,"value":value}),
            Err(error) => serde_json::json!({"ok":false,"error":error}),
        })
        .unwrap_or_default()
    }
}
