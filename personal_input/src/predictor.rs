//! Replaceable one-slot prediction queue, shared by the two platform adapters.
use crate::{learning, store::Store};
use core_engine::assistance::{
    PredictionResponse, ProposalResponse, RequestKey, ScoreRequest, Unavailable, PROTOCOL_VERSION,
};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};
struct Job {
    root: PathBuf,
    request: ScoreRequest,
    right: Option<String>,
}
#[derive(Default)]
struct State {
    job: Option<Job>,
    key: Option<RequestKey>,
    result: Option<(RequestKey, Result<PredictionResponse, Unavailable>)>,
    closed: bool,
    version: Option<String>,
}
pub struct Predictor {
    shared: Arc<(Mutex<State>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Predictor {
    pub fn new() -> std::io::Result<Self> {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("personal-predict".into())
            .spawn(move || loop {
                let job = {
                    let Ok(mut s) = worker.0.lock() else { return };
                    while s.job.is_none() && !s.closed {
                        let Ok(next) = worker.1.wait(s) else { return };
                        s = next;
                    }
                    if s.closed {
                        return;
                    }
                    s.job.take()
                };
                let Some(job) = job else { continue };
                let mut version = None;
                let result = (|| {
                    let right = job.right.ok_or(Unavailable::Capability)?;
                    let model = learning::load(&Store::new(job.root))
                        .map_err(|_| Unavailable::MissingModel)?;
                    version = Some(model.version.clone());
                    let words = model
                        .predict(&job.request.prefix, &right, 16)
                        .into_iter()
                        .filter(|word| {
                            core_engine::assistance::prediction::valid_proposal(
                                &job.request.prefix,
                                word,
                            )
                        })
                        .collect();
                    Ok(PredictionResponse::Proposed(ProposalResponse {
                        protocol: PROTOCOL_VERSION,
                        key: job.request.key,
                        words,
                    }))
                })();
                if let Ok(mut s) = worker.0.lock() {
                    if s.key == Some(job.request.key) {
                        s.version = version;
                        s.result = Some((job.request.key, result));
                    }
                }
            })?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }
    pub fn submit(&self, root: PathBuf, request: ScoreRequest, right: Option<String>) {
        if let Ok(mut s) = self.shared.0.lock() {
            if s.key == Some(request.key) {
                return;
            }
            s.key = Some(request.key);
            s.result = None;
            s.job = Some(Job {
                root,
                request,
                right,
            });
            self.shared.1.notify_one();
        }
    }
    pub fn take(&self, key: RequestKey) -> Option<Result<PredictionResponse, Unavailable>> {
        let mut s = self.shared.0.lock().ok()?;
        if s.result.as_ref().is_some_and(|(k, _)| *k == key) {
            s.result.take().map(|(_, v)| v)
        } else {
            None
        }
    }
    pub fn invalidate(&self) {
        if let Ok(mut s) = self.shared.0.lock() {
            s.key = None;
            s.job = None;
            s.result = None;
        }
    }
    pub fn version(&self) -> Option<String> {
        self.shared.0.lock().ok()?.version.clone()
    }
}
impl Drop for Predictor {
    fn drop(&mut self) {
        if let Ok(mut s) = self.shared.0.lock() {
            s.closed = true;
            s.job = None;
        }
        self.shared.1.notify_one();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
