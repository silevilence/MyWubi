//! One user-local, process-locked append journal. Truncated final records are gaps.
use crate::{Error, Event, PersonalConfig, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub const MAX_BYTES: u64 = 256 * 1024 * 1024;
#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: u64,
    pub events: BTreeMap<String, Event>,
    /// Bounded tombstones retain identities of evicted events, never plaintext.
    pub retired: BTreeMap<String, String>,
    pub gaps: u64,
}
#[derive(Serialize, Deserialize)]
struct Frame {
    events: Vec<Event>,
    dropped: u64,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    pub records: usize,
    pub bytes: u64,
    pub gaps: u64,
    pub generation: u64,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn event_digest(event: &Event) -> Result<String> {
    Ok(digest(&serde_json::to_vec(event)?))
}
impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn beside_config(config: &Path) -> Self {
        Self::new(config.parent().unwrap_or(Path::new(".")).join("personal"))
    }
    pub(crate) fn lock(&self) -> Result<File> {
        fs::create_dir_all(&self.root)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("store.lock"))?;
        file.lock_exclusive()?;
        Ok(file)
    }
    pub(crate) fn load_locked(&self) -> Result<Snapshot> {
        let mut state: Snapshot = match fs::read(self.root.join("snapshot.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Snapshot::default(),
            Err(e) => return Err(e.into()),
        };
        let journal = self
            .root
            .join(format!("journal-{}.jsonl", state.generation));
        if journal.exists() {
            if journal.metadata()?.len() > MAX_BYTES {
                return Err(Error::Invalid("个人数据超过读取上限"));
            }
            let bytes = fs::read(journal)?;
            for line in bytes.split_inclusive(|b| *b == b'\n') {
                if !line.ends_with(b"\n") {
                    state.gaps += 1;
                    break;
                }
                let frame: Frame = serde_json::from_slice(line)?;
                state.gaps += frame.dropped;
                for event in frame.events {
                    state.events.entry(event.id.clone()).or_insert(event);
                }
            }
        }
        Ok(state)
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let _lock = self.lock()?;
        self.load_locked()
    }
    pub fn stats(&self) -> Result<Stats> {
        let state = self.snapshot()?;
        Ok(Stats {
            records: state.events.len(),
            bytes: serde_json::to_vec(&state)?.len() as u64,
            gaps: state.gaps,
            generation: state.generation,
        })
    }
    /// A new snapshot selects a new journal generation in one atomic replace.
    pub(crate) fn replace_locked(&self, mut state: Snapshot) -> Result<()> {
        state.generation += 1;
        core_engine::data::atomic_write(
            &self.root.join("snapshot.json"),
            &serde_json::to_vec(&state)?,
        )?;
        // Old generations are no longer reachable, including after a crash.
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("journal-")
                && name.ends_with(".jsonl")
                && entry.file_type()?.is_file()
            {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
    pub fn append(&self, events: Vec<Event>, dropped: u64, config: &PersonalConfig) -> Result<()> {
        if !config.collect {
            return Ok(());
        }
        for event in &events {
            event.validate().map_err(Error::Invalid)?;
        }
        let _lock = self.lock()?;
        let mut state = self.load_locked()?;
        let events: Vec<_> = events
            .into_iter()
            .filter(|e| !state.events.contains_key(&e.id) && !state.retired.contains_key(&e.id))
            .collect();
        let mut bytes = serde_json::to_vec(&Frame {
            events: events.clone(),
            dropped,
        })?;
        bytes.push(b'\n');
        let journal = self
            .root
            .join(format!("journal-{}.jsonl", state.generation));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal)?;
        let length = file.metadata()?.len();
        if length > 0 {
            use std::io::{Read, Seek, SeekFrom};
            let mut input = File::open(&journal)?;
            input.seek(SeekFrom::End(-1))?;
            let mut last = [0];
            input.read_exact(&mut last)?;
            if last[0] != b'\n' {
                drop(file);
                for event in events {
                    state.events.insert(event.id.clone(), event);
                }
                state.gaps += dropped;
                self.prune(&mut state, config)?;
                return self.replace_locked(state);
            }
        }
        file.write_all(&bytes)?;
        file.sync_data()?;
        drop(file);
        for event in events {
            state.events.insert(event.id.clone(), event);
        }
        state.gaps += dropped;
        let before = state.events.len();
        self.prune(&mut state, config)?;
        if state.events.len() != before
            || length + bytes.len() as u64 > u64::from(config.max_mb) * 1024 * 1024
        {
            self.replace_locked(state)?;
        }
        Ok(())
    }
    fn prune(&self, state: &mut Snapshot, config: &PersonalConfig) -> Result<()> {
        let cutoff = crate::now_ms().saturating_sub(u64::from(config.retention_days) * 86_400_000);
        let limit = u64::from(config.max_mb) * 1024 * 1024;
        let mut size = serde_json::to_vec(state)?.len() as u64;
        let mut order: Vec<_> = state
            .events
            .values()
            .map(|e| (e.time_ms, e.id.clone()))
            .collect();
        order.sort();
        for (time, id) in order {
            if time >= cutoff && size <= limit {
                break;
            }
            if let Some(event) = state.events.remove(&id) {
                let bytes = serde_json::to_vec(&event)?;
                size = size.saturating_sub(bytes.len() as u64);
                state.retired.insert(id, digest(&bytes));
            }
        }
        while state.retired.len() > 100_000 {
            state.retired.pop_first();
        }
        Ok(())
    }
    /// Raw-data clear does not imply deleting knowledge already in a model.
    pub fn clear(&self, models: bool) -> Result<()> {
        let _lock = self.lock()?;
        let old = self.load_locked()?;
        self.replace_locked(Snapshot {
            generation: old.generation,
            ..Default::default()
        })?;
        for name in if models {
            &["training.json", "model.json", "previous-model.json"][..]
        } else {
            &["training.json"][..]
        } {
            match fs::remove_file(self.root.join(name)) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
