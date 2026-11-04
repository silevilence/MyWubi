//! Lazy model owner shared by the desktop service and Android JNI. UI methods
//! only replace bounded state; validation, load, reset and inference run here.
use core_engine::{assistance::*, config::PredictionConfig};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Loading,
    Ready,
    Paused,
    Failed(Unavailable),
}
struct Job {
    request: ScoreRequest,
    deadline: Instant,
}
struct Done {
    key: RequestKey,
    result: Result<ScoreResponse, Unavailable>,
    deadline: Instant,
}
struct State {
    root: PathBuf,
    config: PredictionConfig,
    epoch: u64,
    pending: Option<Job>,
    result: Option<Done>,
    cancel: Option<Cancellation>,
    paused: bool,
    closed: bool,
    reset: bool,
    phase: Phase,
    last_activity: Instant,
}
pub struct Worker {
    shared: Arc<(Mutex<State>, Condvar)>,
}
type Backend = Box<dyn ModelBackend>;
type Loader = fn(&std::path::Path, &PredictionConfig) -> Result<Backend, Unavailable>;
impl Worker {
    pub fn new(root: PathBuf, config: PredictionConfig) -> std::io::Result<Self> {
        Self::with_loader(root, config, |root, config| {
            let name = config
                .model
                .as_ref()
                .and_then(|m| m.directory_name())
                .ok_or(Unavailable::MissingModel)?;
            let package =
                crate::package::validate(&root.join(name)).map_err(|_| Unavailable::Load)?;
            crate::package::load_with_threads(&package, config.threads)
        })
    }
    fn with_loader(root: PathBuf, config: PredictionConfig, load: Loader) -> std::io::Result<Self> {
        let shared = Arc::new((
            Mutex::new(State {
                root,
                config,
                epoch: 0,
                pending: None,
                result: None,
                cancel: None,
                paused: false,
                closed: false,
                reset: false,
                phase: Phase::Idle,
                last_activity: Instant::now(),
            }),
            Condvar::new(),
        ));
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("mywubi-inference".into())
            .spawn(move || run(worker, load))?;
        Ok(Self { shared })
    }
    pub fn configure(&self, config: PredictionConfig) {
        if let Ok(mut s) = self.shared.0.lock() {
            if s.config != config {
                s.config = config;
                s.epoch += 1;
                invalidate(&mut s);
                self.shared.1.notify_one();
            }
        }
    }
    pub fn configure_root(&self, root: PathBuf, config: PredictionConfig) {
        if let Ok(mut s) = self.shared.0.lock() {
            if s.root != root || s.config != config {
                s.root = root;
                s.config = config;
                s.epoch += 1;
                invalidate(&mut s);
                self.shared.1.notify_one();
            }
        }
    }
    pub fn pause(&self, paused: bool) {
        if let Ok(mut s) = self.shared.0.lock() {
            if s.paused != paused {
                s.paused = paused;
                s.epoch += 1;
                invalidate(&mut s);
                self.shared.1.notify_one();
            }
        }
    }
    pub fn invalidate(&self) {
        if let Ok(mut s) = self.shared.0.lock() {
            invalidate(&mut s);
            self.shared.1.notify_one();
        }
    }
    pub fn phase(&self) -> Phase {
        self.shared
            .0
            .lock()
            .map(|s| s.phase)
            .unwrap_or(Phase::Failed(Unavailable::Inference))
    }
    pub fn submit(&self, request: ScoreRequest) -> Result<(), Unavailable> {
        let mut s = self.shared.0.lock().map_err(|_| Unavailable::Inference)?;
        if s.closed || s.paused || !s.config.enabled {
            return Err(Unavailable::Cancelled);
        }
        if s.config.model.is_none() {
            return Err(Unavailable::MissingModel);
        }
        // Reject oversized input before it reaches an allocation-heavy adapter.
        if request.task != Task::Predict
            || request.prefix.chars().count() > 256
            || request.candidates.len() > usize::from(s.config.max_candidates)
            || request
                .candidates
                .iter()
                .any(|c| c.text.chars().count() > 16)
        {
            return Err(Unavailable::Limit);
        }
        if s.pending
            .as_ref()
            .is_some_and(|j| j.request.key == request.key)
        {
            return Ok(());
        }
        s.pending = None;
        s.result = None;
        if let Some(cancel) = &s.cancel {
            cancel.cancel();
        }
        s.last_activity = Instant::now();
        s.pending = Some(Job {
            request,
            deadline: Instant::now() + Duration::from_millis(u64::from(s.config.wait_ms)),
        });
        self.shared.1.notify_one();
        Ok(())
    }
    pub fn take(&self, key: RequestKey) -> Option<Result<ScoreResponse, Unavailable>> {
        let mut s = self.shared.0.lock().ok()?;
        if s.result.as_ref().is_none_or(|done| done.key != key) {
            return None;
        }
        let done = s.result.take()?;
        (done.key == key && Instant::now() < done.deadline).then_some(done.result)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Ok(mut s) = self.shared.0.lock() {
            s.closed = true;
            invalidate(&mut s);
            self.shared.1.notify_one();
        }
    }
}
fn invalidate(s: &mut State) {
    s.pending = None;
    s.result = None;
    s.reset = true;
    if let Some(c) = &s.cancel {
        c.cancel();
    }
}
fn run(shared: Arc<(Mutex<State>, Condvar)>, load: Loader) {
    let mut loaded: Option<(PathBuf, PredictionConfig, ModelSession<Backend>)> = None;
    loop {
        let (job, root, config, epoch, control) = {
            let Ok(mut s) = shared.0.lock() else { return };
            loop {
                if s.closed {
                    return;
                }
                if s.paused || !s.config.enabled || s.config.model.is_none() {
                    s.phase = if s.paused { Phase::Paused } else { Phase::Idle };
                    if loaded.is_some() {
                        drop(s);
                        loaded = None;
                        s = match shared.0.lock() {
                            Ok(s) => s,
                            Err(_) => return,
                        };
                        continue;
                    }
                }
                if s.reset {
                    s.reset = false;
                    drop(s);
                    if let Some((_, _, m)) = &mut loaded {
                        m.reset();
                    }
                    s = match shared.0.lock() {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    continue;
                }
                if !s.paused && s.config.enabled {
                    if let Some(job) = s.pending.take() {
                        let control = ComputeControl {
                            cancellation: Cancellation::default(),
                            deadline: job.deadline,
                        };
                        s.cancel = Some(control.cancellation.clone());
                        break (job, s.root.clone(), s.config.clone(), s.epoch, control);
                    }
                }
                let idle = Duration::from_secs(u64::from(s.config.idle_seconds));
                let remaining = idle.saturating_sub(s.last_activity.elapsed());
                if remaining.is_zero() {
                    s.phase = if s.paused { Phase::Paused } else { Phase::Idle };
                    if loaded.is_some() {
                        drop(s);
                        loaded = None;
                        s = match shared.0.lock() {
                            Ok(s) => s,
                            Err(_) => return,
                        };
                        continue;
                    }
                }
                let wait = if loaded.is_some() {
                    remaining.max(Duration::from_millis(1))
                } else {
                    Duration::from_secs(600)
                };
                s = match shared.1.wait_timeout(s, wait) {
                    Ok((s, _)) => s,
                    Err(_) => return,
                };
            }
        };
        let result = (|| {
            if loaded
                .as_ref()
                .is_none_or(|(path, previous, _)| path != &root || previous != &config)
            {
                if let Ok(mut s) = shared.0.lock() {
                    s.phase = Phase::Loading;
                }
                // A failed replacement leaves the last working instance intact.
                // At most one replacement loads beside it, on this same thread.
                let backend = load(&root, &config)?;
                loaded = Some((root, config.clone(), ModelSession::new(backend)));
            }
            control.check()?;
            loaded
                .as_mut()
                .ok_or(Unavailable::Load)?
                .2
                .score(&job.request, &control)
        })();
        let Ok(mut s) = shared.0.lock() else { return };
        s.cancel = None;
        if s.epoch != epoch || control.cancellation.is_cancelled() || s.closed {
            let changed = s.epoch != epoch;
            s.phase = if !changed && loaded.is_some() {
                Phase::Ready
            } else {
                Phase::Idle
            };
            drop(s);
            continue;
        }
        s.phase = match &result {
            Ok(_) => Phase::Ready,
            Err(Unavailable::Timeout | Unavailable::LowConfidence) => {
                if loaded.is_some() {
                    Phase::Ready
                } else {
                    Phase::Idle
                }
            }
            Err(e) => Phase::Failed(*e),
        };
        if control.check().is_ok() {
            s.result = Some(Done {
                key: job.request.key,
                result,
                deadline: job.deadline,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LOADS: AtomicUsize = AtomicUsize::new(0);
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(Capabilities);
    impl Drop for Fixture {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl ModelBackend for Fixture {
        fn capabilities(&self) -> &Capabilities {
            &self.0
        }
        fn reset(&mut self) {}
        fn score(
            &mut self,
            r: &ScoreRequest,
            c: &ComputeControl,
        ) -> Result<ScoreResponse, Unavailable> {
            std::thread::sleep(Duration::from_millis(10));
            c.check()?;
            Ok(ScoreResponse {
                prediction_accepted: None,
                protocol: 1,
                key: r.key,
                scores: r
                    .candidates
                    .iter()
                    .map(|v| CandidateScore {
                        id: v.id,
                        value: 1.0,
                    })
                    .collect(),
            })
        }
    }
    fn load(path: &std::path::Path, _: &PredictionConfig) -> Result<Backend, Unavailable> {
        if path.ends_with("broken") {
            return Err(Unavailable::Load);
        }
        LOADS.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Fixture(Capabilities {
            protocol: 1,
            tasks: vec![Task::Predict],
            languages: vec!["zh".into()],
            max_context_chars: 256,
            max_candidates: 64,
            max_candidate_chars: 16,
            max_code_chars: 0,
            proposes_words: false,
            incremental: false,
            cooperative_cancel: true,
        })))
    }
    fn request(id: u64) -> ScoreRequest {
        ScoreRequest {
            protocol: 1,
            key: RequestKey {
                edit: core_engine::session::EditToken {
                    session: 1,
                    generation: id,
                    editor: 1,
                    epoch: 1,
                    runtime: 1,
                },
                query: core_engine::candidates::QueryRevision {
                    engine: 0,
                    version: id,
                },
                view: id,
                context: 1,
                model: 1,
                config: 1,
                request: id,
            },
            task: Task::Predict,
            language: "zh".into(),
            prefix: "北".into(),
            code: String::new(),
            candidates: vec![ScoringCandidate {
                id: core_engine::candidates::CandidateId(4),
                text: "京".into(),
            }],
        }
    }
    fn wait(mut ready: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(3);
        while !ready() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn bounded_worker_reuses_model_preserves_foreign_poll_results_and_unloads_on_pause_and_idle() {
        let config = PredictionConfig {
            enabled: true,
            model: Some(core_engine::config::ModelSelection {
                id: "fixture".into(),
                version: "v1".into(),
            }),
            wait_ms: 1000,
            ..Default::default()
        };
        let worker = Worker::with_loader(PathBuf::new(), config.clone(), load).unwrap();
        worker.submit(request(1)).unwrap();
        wait(|| worker.shared.0.lock().unwrap().result.is_some());
        assert!(worker.take(request(2).key).is_none());
        assert!(worker.take(request(1).key).unwrap().is_ok());
        for n in 2..=100 {
            worker.submit(request(n)).unwrap();
        }
        wait(|| {
            worker
                .shared
                .0
                .lock()
                .unwrap()
                .result
                .as_ref()
                .is_some_and(|r| r.key == request(100).key)
        });
        assert!(worker.take(request(100).key).unwrap().is_ok());
        assert_eq!(LOADS.load(Ordering::SeqCst), 1);
        worker.pause(true);
        wait(|| DROPS.load(Ordering::SeqCst) == 1);
        assert_eq!(worker.submit(request(101)), Err(Unavailable::Cancelled));
        worker.pause(false);
        worker.submit(request(102)).unwrap();
        wait(|| worker.shared.0.lock().unwrap().result.is_some());
        {
            worker.shared.0.lock().unwrap().last_activity =
                Instant::now() - Duration::from_secs(601);
            worker.shared.1.notify_one();
        }
        wait(|| DROPS.load(Ordering::SeqCst) == 2);
        assert_eq!(worker.phase(), Phase::Idle);
        for n in 104..204 {
            worker.configure_root(PathBuf::from(format!("store-{n}")), config.clone());
            worker.submit(request(n)).unwrap();
        }
        wait(|| {
            worker
                .shared
                .0
                .lock()
                .unwrap()
                .result
                .as_ref()
                .is_some_and(|r| r.key == request(203).key)
        });
        assert!(worker.take(request(203).key).unwrap().is_ok());
        // Only the latest working model remains after replacement commits.
        assert_eq!(
            LOADS.load(Ordering::SeqCst) - DROPS.load(Ordering::SeqCst),
            1
        );
        let before = LOADS.load(Ordering::SeqCst);
        worker.configure_root(PathBuf::from("broken"), config.clone());
        worker.submit(request(204)).unwrap();
        wait(|| worker.shared.0.lock().unwrap().result.is_some());
        assert!(matches!(
            worker.take(request(204).key).unwrap(),
            Err(Unavailable::Load)
        ));
        assert_eq!(
            LOADS.load(Ordering::SeqCst) - DROPS.load(Ordering::SeqCst),
            1
        );
        worker.configure_root(PathBuf::from("store-203"), config.clone());
        worker.submit(request(205)).unwrap();
        wait(|| worker.shared.0.lock().unwrap().result.is_some());
        assert!(worker.take(request(205).key).unwrap().is_ok());
        assert_eq!(
            LOADS.load(Ordering::SeqCst),
            before,
            "last working model remains reusable after a failed switch"
        );
        let mut disabled = config;
        disabled.enabled = false;
        worker.configure(disabled);
        assert_eq!(worker.submit(request(103)), Err(Unavailable::Cancelled));
    }
}
