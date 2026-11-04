//! One running computation and one replaceable pending request, never a FIFO backlog.
use core_engine::assistance::*;
use std::{
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct Job {
    request: ScoreRequest,
    control: ComputeControl,
}
struct Completed {
    request: ScoreRequest,
    result: Result<ScoreResponse, Unavailable>,
    deadline: Instant,
}
#[derive(Default)]
struct State {
    latest: Option<RequestKey>,
    pending: Option<Job>,
    active: Option<Cancellation>,
    completed: Option<Completed>,
    closed: bool,
    reset_requested: bool,
    stats: Statistics,
}

/// Counts include discarded work, without retaining text or per-request logs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Statistics {
    pub submitted: u64,
    pub replaced: u64,
    pub computed: u64,
    pub discarded: u64,
    pub delivered: u64,
}

/// Nonblocking owner. Drop requests cancellation and detaches the join handle;
/// noncooperative native work requires process termination for immediate release.
pub struct Scheduler {
    shared: Arc<(Mutex<State>, Condvar)>,
    worker: Option<JoinHandle<()>>,
    capabilities: Capabilities,
}

impl Scheduler {
    pub fn new<B: ModelBackend + 'static>(backend: B) -> Result<Self, std::io::Error> {
        let capabilities = backend.capabilities().clone();
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker_state = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("mywubi-model".into())
            .spawn(move || run(backend, worker_state))?;
        Ok(Self {
            shared,
            worker: Some(worker),
            capabilities,
        })
    }

    /// Monotonic bounded deadline; validation precedes enqueueing and allocations.
    pub fn submit(&self, request: ScoreRequest, budget: Duration) -> Result<(), Unavailable> {
        request.validate(&self.capabilities)?;
        if budget.is_zero() || budget > Duration::from_secs(1) {
            return Err(Unavailable::Limit);
        }
        let mut state = self.shared.0.lock().map_err(|_| Unavailable::Inference)?;
        if state.closed {
            return Err(Unavailable::Cancelled);
        }
        // Keys are unique for immutable requests. Identical keys coalesce even in flight.
        if state.latest == Some(request.key) {
            return Ok(());
        }
        if let Some(cancel) = state.active.as_ref() {
            cancel.cancel();
        }
        if state.pending.is_some() {
            state.stats.replaced += 1;
        }
        state.stats.submitted += 1;
        state.latest = Some(request.key);
        state.completed = None;
        state.pending = Some(Job {
            request,
            control: ComputeControl {
                cancellation: Cancellation::default(),
                deadline: Instant::now() + budget,
            },
        });
        self.shared.1.notify_one();
        Ok(())
    }

    /// Call on new input, submission, focus loss, model replacement or feature disable.
    pub fn invalidate(&self) {
        if let Ok(mut state) = self.shared.0.lock() {
            state.latest = None;
            state.pending = None;
            state.completed = None;
            state.reset_requested = true;
            if let Some(cancel) = state.active.as_ref() {
                cancel.cancel();
            }
            self.shared.1.notify_one();
        }
    }

    /// Delivery-time validation is essential even if computation finished before timeout.
    pub fn take(&self, current: RequestKey) -> Option<Result<ScoreResponse, Unavailable>> {
        let mut state = self.shared.0.lock().ok()?;
        let done = state.completed.take()?;
        if state.latest != Some(current)
            || done.request.key != current
            || Instant::now() >= done.deadline
        {
            state.stats.discarded += 1;
            return None;
        }
        let result = done.result.and_then(|response| {
            response.validate(&done.request, current)?;
            Ok(response)
        });
        if result.is_ok() {
            state.stats.delivered += 1;
        }
        Some(result)
    }
    pub fn statistics(&self) -> Statistics {
        self.shared.0.lock().map(|s| s.stats).unwrap_or_default()
    }
    /// Join only off the UI/input thread. A noncooperative backend may exceed its budget.
    pub fn shutdown(mut self) -> thread::Result<()> {
        self.close();
        if let Some(worker) = self.worker.take() {
            worker.join()
        } else {
            Ok(())
        }
    }
    fn close(&self) {
        if let Ok(mut state) = self.shared.0.lock() {
            state.closed = true;
            state.pending = None;
            state.completed = None;
            if let Some(c) = state.active.as_ref() {
                c.cancel();
            }
            self.shared.1.notify_one();
        }
    }
}
impl Drop for Scheduler {
    fn drop(&mut self) {
        self.close();
    }
}

fn run<B: ModelBackend>(backend: B, shared: Arc<(Mutex<State>, Condvar)>) {
    let mut model = ModelSession::new(backend);
    loop {
        let job = {
            let Ok(mut state) = shared.0.lock() else {
                return;
            };
            while state.pending.is_none() && !state.closed && !state.reset_requested {
                let Ok(next) = shared.1.wait(state) else {
                    return;
                };
                state = next;
            }
            if state.closed {
                return;
            }
            if state.reset_requested {
                state.reset_requested = false;
                // Reset off the input thread, without holding its queue lock.
                drop(state);
                model.reset();
                continue;
            }
            let Some(job) = state.pending.take() else {
                continue;
            };
            state.active = Some(job.control.cancellation.clone());
            job
        };
        let result = model.score(&job.request, &job.control);
        let Ok(mut state) = shared.0.lock() else {
            return;
        };
        state.active = None;
        state.stats.computed += 1;
        if !state.closed && state.latest == Some(job.request.key) && job.control.check().is_ok() {
            state.completed = Some(Completed {
                request: job.request,
                result,
                deadline: job.control.deadline,
            });
        } else {
            state.stats.discarded += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    struct Controlled {
        caps: Capabilities,
        started: mpsc::Sender<u64>,
        release: mpsc::Receiver<()>,
        resets: Option<mpsc::Sender<()>>,
    }
    impl ModelBackend for Controlled {
        fn capabilities(&self) -> &Capabilities {
            &self.caps
        }
        fn reset(&mut self) {
            if let Some(resets) = &self.resets {
                let _ = resets.send(());
            }
        }
        fn score(
            &mut self,
            r: &ScoreRequest,
            _: &ComputeControl,
        ) -> Result<ScoreResponse, Unavailable> {
            self.started.send(r.key.request).unwrap();
            self.release.recv().unwrap();
            Ok(ScoreResponse {
                protocol: 1,
                key: r.key,
                scores: r
                    .candidates
                    .iter()
                    .map(|c| CandidateScore {
                        id: c.id,
                        value: 1.,
                    })
                    .collect(),
            })
        }
    }
    fn request(n: u64) -> ScoreRequest {
        let mut r: ScoreRequest =
            serde_json::from_str(include_str!("../evaluation/request.json")).unwrap();
        r.key.request = n;
        r
    }
    #[test]
    fn continuous_input_replaces_pending_work_and_never_delivers_cancelled_results() {
        let (start, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let scheduler = Scheduler::new(Controlled {
            caps: Capabilities {
                protocol: 1,
                tasks: vec![Task::Rank],
                languages: vec!["zh".into()],
                max_context_chars: 256,
                max_candidates: 64,
                max_candidate_chars: 16,
                max_code_chars: 64,
                proposes_words: false,
                incremental: false,
                cooperative_cancel: false,
            },
            started: start,
            release: wait,
            resets: None,
        })
        .unwrap();
        scheduler
            .submit(request(1), Duration::from_secs(1))
            .unwrap();
        assert_eq!(started.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
        for n in 2..=100 {
            scheduler
                .submit(request(n), Duration::from_secs(1))
                .unwrap();
        }
        assert!(scheduler.take(request(1).key).is_none());
        release.send(()).unwrap();
        assert_eq!(started.recv_timeout(Duration::from_secs(2)).unwrap(), 100);
        scheduler.invalidate();
        release.send(()).unwrap();
        wait_computed(&scheduler, 2);
        assert!(scheduler.take(request(100).key).is_none());
        assert_eq!(scheduler.statistics().replaced, 98);
        assert_eq!(scheduler.statistics().discarded, 2);
        scheduler.shutdown().unwrap();
    }

    fn wait_computed(scheduler: &Scheduler, count: u64) {
        let end = Instant::now() + Duration::from_secs(2);
        while scheduler.statistics().computed < count {
            assert!(Instant::now() < end, "worker failed to finish");
            thread::yield_now();
        }
    }

    #[test]
    fn duplicate_requests_coalesce_and_results_expire_before_ui_delivery() {
        let (start, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let scheduler = Scheduler::new(Controlled {
            caps: Capabilities {
                protocol: 1,
                tasks: vec![Task::Rank],
                languages: vec!["zh".into()],
                max_context_chars: 256,
                max_candidates: 64,
                max_candidate_chars: 16,
                max_code_chars: 64,
                proposes_words: false,
                incremental: false,
                cooperative_cancel: false,
            },
            started: start,
            release: wait,
            resets: None,
        })
        .unwrap();
        scheduler
            .submit(request(1), Duration::from_millis(80))
            .unwrap();
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        scheduler
            .submit(request(1), Duration::from_millis(80))
            .unwrap();
        release.send(()).unwrap();
        wait_computed(&scheduler, 1);
        thread::sleep(Duration::from_millis(90));
        assert!(scheduler.take(request(1).key).is_none());
        assert_eq!(scheduler.statistics().submitted, 1);
        assert_eq!(scheduler.statistics().delivered, 0);
        scheduler.shutdown().unwrap();
    }

    #[test]
    fn invalidating_an_idle_worker_clears_its_private_context_without_new_input() {
        let (start, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let (reset, resets) = mpsc::channel();
        let scheduler = Scheduler::new(Controlled {
            caps: Capabilities {
                protocol: 1,
                tasks: vec![Task::Rank],
                languages: vec!["zh".into()],
                max_context_chars: 256,
                max_candidates: 64,
                max_candidate_chars: 16,
                max_code_chars: 64,
                proposes_words: false,
                incremental: true,
                cooperative_cancel: false,
            },
            started: start,
            release: wait,
            resets: Some(reset),
        })
        .unwrap();
        scheduler
            .submit(request(1), Duration::from_secs(1))
            .unwrap();
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        resets.recv_timeout(Duration::from_secs(2)).unwrap();
        release.send(()).unwrap();
        wait_computed(&scheduler, 1);
        scheduler.invalidate();
        resets.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(scheduler.take(request(1).key).is_none());
        scheduler.shutdown().unwrap();
    }
}
