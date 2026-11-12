//! Bounded producer; all filesystem work is on its writer thread.
use crate::{Change, Context, Event, Evidence, PersonalConfig, Result, FORMAT};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    thread::JoinHandle,
};

pub struct Recorder {
    source: String,
    sequence: u64,
    session: String,
    tx: Option<SyncSender<(u64, Event)>>,
    epoch: Arc<AtomicU64>,
    enabled: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}
impl Recorder {
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let source = crate::identity()?;
        let (tx, rx) = mpsc::sync_channel::<(u64, Event)>(128);
        let epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = epoch.clone();
        let enabled = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let gate = enabled.clone();
        let loss = dropped.clone();
        let worker = std::thread::Builder::new()
            .name("personal-writer".into())
            .spawn(move || {
                let store = crate::store::Store::beside_config(&config_path);
                while let Ok(first) = rx.recv() {
                    let current = worker_epoch.load(Ordering::Acquire);
                    let mut batch = Vec::new();
                    for (generation, event) in std::iter::once(first).chain(rx.try_iter().take(63))
                    {
                        if generation == current {
                            batch.push(event);
                        }
                    }
                    let config = core_engine::Config::load(&config_path)
                        .map(|c| c.personal)
                        .unwrap_or_default();
                    if !gate.load(Ordering::Acquire) || !config.collect || batch.is_empty() {
                        continue;
                    }
                    let n = batch.len() as u64;
                    let missing = loss.swap(0, Ordering::AcqRel);
                    if store
                        .append_if(batch, missing, &config, || {
                            gate.load(Ordering::Acquire)
                                && worker_epoch.load(Ordering::Acquire) == current
                        })
                        .is_err()
                    {
                        loss.fetch_add(n + missing, Ordering::Relaxed);
                    }
                }
            })?;
        Ok(Self {
            session: source.clone(),
            source,
            sequence: 0,
            tx: Some(tx),
            enabled,
            epoch,
            dropped,
            worker: Some(worker),
        })
    }
    pub fn configure(&mut self, config: &PersonalConfig, allowed: bool, session: &str) {
        if !config.collect || !allowed || self.session != session {
            self.epoch.fetch_add(1, Ordering::AcqRel);
        }
        self.enabled
            .store(config.collect && allowed, Ordering::Release);
        self.session = session.to_owned();
    }
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
    /// Build observations only after checking enabled; disabled use stores no text.
    pub fn record(
        &mut self,
        mode: &str,
        before: Context,
        after: Option<Context>,
        change: Change,
        evidence: Evidence,
    ) -> Option<String> {
        if !self.enabled() {
            return None;
        }
        self.sequence += 1;
        let id = format!("{}:{}", self.source, self.sequence);
        let event = Event {
            format: FORMAT,
            id: id.clone(),
            source: self.source.clone(),
            session: self.session.clone(),
            sequence: self.sequence,
            time_ms: crate::now_ms(),
            mode: mode.into(),
            evidence,
            before,
            after,
            change,
            extensions: Default::default(),
        };
        if event.validate().is_err() || serde_json::to_vec(&event).is_ok_and(|b| b.len() > 65536) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        if self
            .tx
            .as_ref()?
            .try_send((self.epoch.load(Ordering::Acquire), event))
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        Some(id)
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        self.tx.take();
        // Drain authorized events on ordinary shutdown. Explicit privacy/consent
        // changes have already revoked the gate and discard their queued data.
        // Joining also keeps a DLL thread from executing unloaded code.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
