//! One cooperative training worker. No model dependency is required to collect.
use crate::{learning, store::Store};
pub fn process_lease(config: &std::path::Path) -> crate::Result<std::fs::File> {
    use fs2::FileExt;
    let store = Store::beside_config(config);
    std::fs::create_dir_all(&store.root)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(store.root.join("background.lock"))?;
    file.try_lock_exclusive()?;
    Ok(file)
}
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Clone, serde::Serialize)]
pub struct Status {
    pub phase: String,
    pub reason: String,
    pub elapsed_ms: u64,
    pub last_update: u64,
    pub threads: u8,
    pub batch_seconds: u64,
    pub max_model_mb: u8,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            phase: "waiting".into(),
            reason: "等待配置".into(),
            elapsed_ms: 0,
            last_update: 0,
            threads: 1,
            batch_seconds: learning::BATCH_SECONDS,
            max_model_mb: 8,
        }
    }
}
struct State {
    config: Option<PathBuf>,
    allowed: bool,
    last_activity: Instant,
    last_attempt: Instant,
    status: Status,
    closed: bool,
}
pub struct Worker {
    shared: Arc<(Mutex<State>, Condvar)>,
    cancel: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Worker {
    pub fn new() -> std::io::Result<Self> {
        let shared = Arc::new((
            Mutex::new(State {
                config: None,
                allowed: false,
                last_activity: Instant::now(),
                last_attempt: Instant::now() - Duration::from_secs(300),
                status: Status::default(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let cancel = Arc::new(AtomicBool::new(false));
        let state = shared.clone();
        let signal = cancel.clone();
        let thread = std::thread::Builder::new()
            .name("personal-learning".into())
            .spawn(move || loop {
                // File I/O stays outside the lock touched by the input thread.
                let (config, status) = {
                    let Ok(guard) = state.0.lock() else { return };
                    let Ok((guard, _)) = state.1.wait_timeout(guard, Duration::from_secs(1)) else {
                        return;
                    };
                    if guard.closed {
                        return;
                    }
                    let Some(config) = guard.config.clone() else {
                        continue;
                    };
                    (config, guard.status.clone())
                };
                let store = Store::beside_config(&config);
                if let Ok(mut value) = serde_json::to_value(status) {
                    value["observed_ms"] = crate::now_ms().into();
                    if let Ok(bytes) = serde_json::to_vec(&value) {
                        let _ = core_engine::data::atomic_write(
                            &store.root.join("learning-status.json"),
                            &bytes,
                        );
                    }
                }
                let enabled = core_engine::Config::load(&config).is_ok_and(|c| c.personal.learn);
                let request = store.root.join("update-request");
                let requested = request.exists();
                {
                    let Ok(mut guard) = state.0.lock() else {
                        return;
                    };
                    if guard.closed {
                        return;
                    }
                    if guard.config.as_ref() != Some(&config) {
                        continue;
                    }
                    let reason = if !enabled {
                        "后台学习已暂停"
                    } else if !guard.allowed {
                        "等待电量、温度、内存或后台条件"
                    } else if guard.last_activity.elapsed() < Duration::from_secs(30) {
                        "等待输入空闲 30 秒"
                    } else if !requested && guard.last_attempt.elapsed() < Duration::from_secs(300)
                    {
                        "等待学习间隔 5 分钟"
                    } else {
                        ""
                    };
                    if !reason.is_empty() {
                        guard.status.phase = "waiting".into();
                        guard.status.reason = reason.into();
                        continue;
                    }
                    guard.status.phase = "training".into();
                    guard.status.reason = "".into();
                    guard.last_attempt = Instant::now();
                    signal.store(false, Ordering::Release);
                }
                if requested {
                    let _ = std::fs::remove_file(request);
                }
                let start = Instant::now();
                let result = learning::train(&Store::beside_config(&config), &signal);
                if let Ok(mut guard) = state.0.lock() {
                    guard.status.elapsed_ms = start.elapsed().as_millis() as u64;
                    match result {
                        Ok(model) => {
                            guard.status.phase = "ready".into();
                            guard.status.last_update = model.created_ms;
                            guard.status.reason = "".into();
                        }
                        Err(e) => {
                            guard.status.phase = "waiting".into();
                            guard.status.reason = e.to_string();
                        }
                    }
                }
            })?;
        Ok(Self {
            shared,
            cancel,
            thread: Some(thread),
        })
    }
    pub fn update(&self, config: PathBuf, allowed: bool, busy: bool) {
        if !allowed || busy {
            self.cancel.store(true, Ordering::Release);
        }
        if let Ok(mut state) = self.shared.0.lock() {
            state.config = Some(config);
            state.allowed = allowed;
            if busy {
                state.last_activity = Instant::now();
            }
        }
        self.shared.1.notify_one();
    }
    pub fn status(&self) -> Status {
        self.shared
            .0
            .lock()
            .map(|s| s.status.clone())
            .unwrap_or_default()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Ok(mut state) = self.shared.0.lock() {
            state.closed = true;
        }
        self.shared.1.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(windows)]
pub fn start_desktop(
    executable: &std::path::Path,
    config: &std::path::Path,
) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(executable)
        .arg("--personal-worker")
        .arg(config)
        .creation_flags(0x08000000)
        .spawn()?;
    Ok(())
}
