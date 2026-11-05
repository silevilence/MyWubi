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
        let mut module = [0u16; 32768];
        let length = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleFileNameW(
                Some(windows::Win32::Foundation::HMODULE(
                    crate::module_handle() as *mut _
                )),
                &mut module,
            )
        };
        if length == 0 || length as usize >= module.len() {
            return Err(std::io::ErrorKind::NotFound.into());
        }
        let program = PathBuf::from(String::from_utf16_lossy(&module[..length as usize]))
            .with_file_name("model-worker.exe");
        let thread = std::thread::Builder::new()
            .name("mywubi-model-ipc".into())
            .spawn(move || run(worker, program))?;
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
fn run(shared: Arc<(Mutex<State>, Condvar)>, program: PathBuf) {
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
                std::process::Command::new(&program)
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
