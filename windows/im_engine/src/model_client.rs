//! Replaceable one-slot IPC client. Model code lives only in model-worker.exe.
use core_engine::{assistance::*, config::PredictionConfig};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
struct Job {
    root: PathBuf,
    config: PredictionConfig,
    request: Option<ScoreRequest>,
}
#[derive(Default)]
struct State {
    pending: Option<Job>,
    configuration: Option<(PathBuf, PredictionConfig)>,
    prepare: bool,
    current: Option<RequestKey>,
    result: Option<(RequestKey, Result<PredictionResponse, Unavailable>)>,
    clear: bool,
    release: bool,
    released: bool,
    closed: bool,
}
pub(crate) struct Client {
    shared: Arc<(Mutex<State>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Client {
    pub fn new() -> std::io::Result<Self> {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("mywubi-model-ipc".into())
            .spawn(move || run(worker))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }
    pub fn prepare(&self, root: PathBuf, config: PredictionConfig) {
        if let Ok(mut s) = self.shared.0.lock() {
            if s.configuration
                .as_ref()
                .is_none_or(|(p, c)| p != &root || c != &config)
            {
                s.configuration = Some((root, config));
                s.prepare = true;
                s.released = false;
                self.shared.1.notify_one();
            }
        }
    }
    pub fn submit(&self, root: PathBuf, config: PredictionConfig, request: ScoreRequest) {
        if let Ok(mut s) = self.shared.0.lock() {
            if s.current == Some(request.key) {
                return;
            }
            s.current = Some(request.key);
            s.released = false;
            s.result = None;
            s.pending = Some(Job {
                root,
                config,
                request: Some(request),
            });
            self.shared.1.notify_one();
        }
    }
    pub fn invalidate(&self) {
        if let Ok(mut s) = self.shared.0.lock() {
            if s.current.take().is_some() {
                s.pending = None;
                s.result = None;
                s.clear = true;
                self.shared.1.notify_one();
            }
        }
    }
    pub fn release(&self) {
        self.invalidate();
        if let Ok(mut s) = self.shared.0.lock() {
            s.configuration = None;
            s.prepare = false;
            if !s.released {
                s.release = true;
                s.released = true;
                self.shared.1.notify_one();
            }
        }
    }
    pub fn take(&self, key: RequestKey) -> Option<Result<PredictionResponse, Unavailable>> {
        let mut s = self.shared.0.lock().ok()?;
        if s.result.as_ref().is_some_and(|(k, _)| *k == key) {
            s.result.take().map(|(_, r)| r)
        } else {
            None
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Ok(mut s) = self.shared.0.lock() {
            s.closed = true;
            s.pending = None;
            self.shared.1.notify_one();
        }
        // Deactivation must join before the injected DLL can unload. All pipe
        // calls have a 300ms bound; this never runs on the ordinary key path.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn exchange(value: serde_json::Value) -> Result<serde_json::Value, Unavailable> {
    let bytes = serde_json::to_vec(&value).map_err(|_| Unavailable::Protocol)?;
    let response =
        model_ipc::exchange(&bytes, Duration::from_millis(300)).map_err(|_| Unavailable::Load)?;
    let response: serde_json::Value =
        serde_json::from_slice(&response).map_err(|_| Unavailable::Protocol)?;
    if response["ok"] != true {
        return Err(Unavailable::Inference);
    }
    Ok(response["value"].clone())
}
fn run(shared: Arc<(Mutex<State>, Condvar)>) {
    use std::os::windows::process::CommandExt;
    let mut last_launch = Instant::now() - Duration::from_secs(2);
    loop {
        let job = {
            let Ok(mut s) = shared.0.lock() else { return };
            while s.pending.is_none() && !s.prepare && !s.clear && !s.release && !s.closed {
                s = match shared.1.wait(s) {
                    Ok(s) => s,
                    Err(_) => return,
                };
            }
            if s.closed {
                return;
            }
            if s.release {
                s.release = false;
                drop(s);
                let _ = exchange(serde_json::json!({"op":"Pause","paused":true}));
                continue;
            }
            if s.clear {
                s.clear = false;
                drop(s);
                let _ = exchange(serde_json::json!({"op":"Invalidate"}));
                continue;
            }
            if let Some(job) = s.pending.take() {
                job
            } else {
                s.prepare = false;
                let Some((root, config)) = &s.configuration else {
                    continue;
                };
                Job {
                    root: root.clone(),
                    config: config.clone(),
                    request: None,
                }
            }
        };
        let key = job.request.as_ref().map(|r| r.key);
        let deadline = Instant::now()
            + if key.is_some() {
                Duration::from_millis(u64::from(job.config.wait_ms))
            } else {
                Duration::from_secs(5)
            };
        let result = (|| {
            let configure = serde_json::json!({"op":if key.is_some() {"Configure"} else {"Prepare"},"root":job.root,"config":job.config});
            if exchange(configure.clone()).is_err() {
                if last_launch.elapsed() < Duration::from_secs(1) {
                    return Err(Unavailable::Load);
                }
                last_launch = Instant::now();
                let directory = job
                    .config
                    .model
                    .as_ref()
                    .and_then(|m| m.directory_name())
                    .ok_or(Unavailable::MissingModel)?;
                let capability = runtime_components::model_capability(&job.root, &directory)
                    .map_err(|_| Unavailable::MissingModel)?;
                let config_path = job.root.with_file_name("config.toml");
                let requested = match job.config.device {
                    core_engine::config::ModelDevice::Vulkan => "vulkan",
                    core_engine::config::ModelDevice::Cuda => "cuda",
                    _ => capability,
                };
                let program = runtime_components::worker(&config_path, requested)
                    .or_else(|_| runtime_components::worker(&config_path, capability))
                    .map_err(|_| Unavailable::Load)?;
                std::process::Command::new(&program)
                    .current_dir(program.parent().ok_or(Unavailable::Load)?)
                    .creation_flags(0x08000000)
                    .spawn()
                    .map_err(|_| Unavailable::Load)?;
                loop {
                    if !shared.0.lock().is_ok_and(|s| {
                        !s.closed
                            && (if key.is_some() {
                                s.current == key
                            } else {
                                s.configuration
                                    .as_ref()
                                    .is_some_and(|(p, c)| p == &job.root && c == &job.config)
                            })
                    }) {
                        return Err(Unavailable::Cancelled);
                    }
                    if Instant::now() >= deadline {
                        return Err(Unavailable::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                    if exchange(configure.clone()).is_ok() {
                        break;
                    }
                }
            }
            let Some(request) = job.request else {
                return Err(Unavailable::Empty);
            };
            exchange(serde_json::json!({"op":"Pause","paused":false}))?;
            exchange(serde_json::json!({"op":"Submit","request":request}))?;
            loop {
                if Instant::now() >= deadline {
                    return Err(Unavailable::Timeout);
                }
                if !shared.0.lock().is_ok_and(|s| {
                    !s.closed
                        && (if key.is_some() {
                            s.current == key
                        } else {
                            s.configuration
                                .as_ref()
                                .is_some_and(|(p, c)| p == &job.root && c == &job.config)
                        })
                }) {
                    return Err(Unavailable::Cancelled);
                }
                let result = exchange(serde_json::json!({"op":"Poll","key":key}))?;
                if !result.is_null() {
                    return serde_json::from_value(result).map_err(|_| Unavailable::Protocol)?;
                }
                std::thread::sleep(Duration::from_millis(8));
            }
        })();
        if let Ok(mut s) = shared.0.lock() {
            if let Some(key) = key.filter(|key| !s.closed && s.current == Some(*key)) {
                s.result = Some((key, result));
            }
        }
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use core_engine::config::{ModelLoading, ModelSelection};
    fn status() -> Option<serde_json::Value> {
        model_ipc::exchange(br#"{"op":"Status"}"#, Duration::from_millis(100))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    }
    fn ready() -> u32 {
        let start = Instant::now();
        loop {
            if let Some(reply) = status() {
                if reply["value"] == "Ready" {
                    return reply["process_id"].as_u64().unwrap() as u32;
                }
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "model service failed to prepare"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn stop_owned(pid: u32) {
        use std::os::windows::process::CommandExt;
        assert!(std::process::Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(0x08000000)
            .status()
            .unwrap()
            .success());
    }
    #[test]
    #[ignore = "real isolated service; set MYWUBI_TEST_MODEL_STORE and MYWUBI_TEST_WORKER"]
    fn active_client_recovers_from_service_crash_and_reactivation_shares_weights() {
        assert!(status().is_none(), "run with no existing service");
        let root = PathBuf::from(std::env::var("MYWUBI_TEST_MODEL_STORE").unwrap());
        let program = PathBuf::from(std::env::var("MYWUBI_TEST_WORKER").unwrap());
        std::fs::copy(
            program,
            std::env::current_exe()
                .unwrap()
                .with_file_name("model-worker.exe"),
        )
        .unwrap();
        let config = PredictionConfig {
            enabled: true,
            loading: ModelLoading::PreloadKeep,
            wait_ms: 1000,
            max_candidates: 1,
            model: Some(ModelSelection {
                id: "qwen25-words".into(),
                version: "v1".into(),
            }),
            ..Default::default()
        };
        let client = Client::new().unwrap();
        client.prepare(root.clone(), config.clone());
        let original = ready();
        stop_owned(original);
        let mut request: ScoreRequest =
            serde_json::from_str(include_str!("../../../model_input/evaluation/request.json"))
                .unwrap();
        request.task = Task::Predict;
        request.prefix = "北京".into();
        request.code.clear();
        request.candidates.clear();
        let start = Instant::now();
        client.submit(root.clone(), config.clone(), request.clone());
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "input was blocked by restart"
        );
        let replacement = ready();
        assert_ne!(original, replacement);
        client.invalidate();
        request.key.request += 1;
        client.submit(root.clone(), config.clone(), request.clone());
        let start = Instant::now();
        let response = loop {
            if let Some(response) = client.take(request.key) {
                break response.unwrap();
            }
            assert!(
                start.elapsed() < Duration::from_millis(1000),
                "recovered model response missed budget"
            );
            std::thread::sleep(Duration::from_millis(2));
        };
        assert!(matches!(response,PredictionResponse::Proposed(r) if r.words==["市"]));
        drop(client);
        let reactivated = Client::new().unwrap();
        reactivated.prepare(root, config);
        assert_eq!(
            ready(),
            replacement,
            "reactivation created a duplicate service"
        );
        drop(reactivated);
        stop_owned(replacement);
    }
}
