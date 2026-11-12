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
    tx: Option<SyncSender<Event>>,
    enabled: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}
impl Recorder {
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let source = crate::identity()?;
        let (tx, rx) = mpsc::sync_channel::<Event>(128);
        let enabled = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let gate = enabled.clone();
        let loss = dropped.clone();
        let worker = std::thread::Builder::new()
            .name("personal-writer".into())
            .spawn(move || {
                let store = crate::store::Store::beside_config(&config_path);
                while let Ok(first) = rx.recv() {
                    let mut batch = vec![first];
                    batch.extend(rx.try_iter().take(63));
                    let config = core_engine::Config::load(&config_path)
                        .map(|c| c.personal)
                        .unwrap_or_default();
                    if !gate.load(Ordering::Acquire) || !config.collect {
                        continue;
                    }
                    let n = batch.len() as u64;
                    let missing = loss.swap(0, Ordering::AcqRel);
                    if store.append(batch, missing, &config).is_err() {
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
            dropped,
            worker: Some(worker),
        })
    }
    pub fn configure(&mut self, config: &PersonalConfig, allowed: bool, session: &str) {
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
        if self.tx.as_ref()?.try_send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        Some(id)
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        self.enabled.store(false, Ordering::Release);
        self.tx.take();
        // Closing an IME must never wait for disk IO. The bounded worker drains
        // revoked messages and exits after the sender closes.
        self.worker.take();
    }
}
