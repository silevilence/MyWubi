//! Local counts of confirmed dictionary selections, scoped to the active table.
use crate::personal::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrequencyConfig {
    pub record: bool,
    pub rank: bool,
}

pub type Counts = BTreeMap<String, BTreeMap<String, u64>>;

pub fn store_path(config: &Path, table: &Path) -> Option<PathBuf> {
    let private = crate::config_path::personal_store_path(config)?;
    let table = table.canonicalize().unwrap_or_else(|_| table.to_owned());
    let digest = Sha256::digest(table.as_os_str().as_encoded_bytes());
    Some(
        private
            .with_file_name("frequency")
            .join(format!("{digest:x}.json")),
    )
}

pub fn counts(path: &Path) -> io::Result<Counts> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Counts::new()),
        Err(e) => Err(e),
    }
}

pub(crate) fn lock_file(path: &Path) -> io::Result<fs::File> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_added_extension("lock"))?;
    file.lock()?;
    Ok(file)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub code: String,
    pub word: String,
}

fn update(path: &Path, add: Option<&Choice>, remove: Option<&Choice>) -> io::Result<()> {
    if add == remove {
        return Ok(());
    }
    let _lock = lock_file(path)?;
    let mut totals = counts(path)?;
    if let Some(key) = remove {
        if let Some(words) = totals.get_mut(&key.code) {
            if let Some(count) = words.get_mut(&key.word) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    words.remove(&key.word);
                }
            }
            if words.is_empty() {
                totals.remove(&key.code);
            }
        }
    }
    if let Some(key) = add {
        let n = totals
            .entry(key.code.clone())
            .or_default()
            .entry(key.word.clone())
            .or_default();
        *n = n.saturating_add(1);
    }
    crate::data::atomic_write(path, &serde_json::to_vec(&totals)?)
}

#[derive(Clone)]
struct Selection {
    key: Choice,
    before: Context,
    remaining: String,
    selected: bool,
}

impl Selection {
    fn new(key: Choice, before: Context) -> Self {
        Self {
            remaining: key.word.clone(),
            key,
            before,
            selected: false,
        }
    }

    /// Both hosts expose bounded context. Compare the same suffix at the left edge.
    fn at(&self, next: &Context, inserted: &str) -> bool {
        let (Some(left), Some(actual)) = (&self.before.left, &next.left) else {
            return false;
        };
        if self.before.right.is_none() || self.before.right != next.right || next.discontinuity {
            return false;
        }
        let expected = format!("{left}{inserted}");
        if expected != *actual
            && !(next.truncated
                && actual.encode_utf16().count() >= 511
                && expected.ends_with(actual))
        {
            return false;
        }
        match (&self.before.selection, &next.selection) {
            (Some(before), Some(after))
                if before.absolute && after.absolute && before.unit == after.unit =>
            {
                let len = match before.unit {
                    crate::personal::CoordinateUnit::Utf16 => inserted.encode_utf16().count(),
                    crate::personal::CoordinateUnit::UnicodeScalar => inserted.chars().count(),
                };
                before.start.min(before.end) + len as i64 == after.start.min(after.end)
            }
            _ => true,
        }
    }
}

/// Context and the last edit live only in their editor session. Disk contains aggregate counts.
#[derive(Default)]
pub(crate) struct Tracker {
    context: Option<Context>,
    waiting: Option<Selection>,
    previous: Option<Selection>,
}

impl Tracker {
    pub fn committed(&mut self, key: Option<Choice>) {
        self.waiting = key
            .zip(self.context.clone())
            .map(|(key, before)| Selection::new(key, before));
    }

    pub fn observe(&mut self, path: &Path, next: Context) -> io::Result<()> {
        let empty = next.selected.as_deref() == Some("");
        if next.discontinuity
            || next.left.is_none()
            || next.right.is_none()
            || next.selected.is_none()
        {
            *self = Self::default();
            return Ok(());
        }
        if let Some(new) = self.waiting.take() {
            if empty && new.at(&next, &new.key.word) {
                let replaced = self
                    .previous
                    .as_ref()
                    .filter(|old| old.selected && old.at(&new.before, ""));
                update(path, Some(&new.key), replaced.map(|old| &old.key))?;
                self.previous = Some(new);
                self.context = Some(next);
                return Ok(());
            }
        }
        if let Some(mut old) = self.previous.take() {
            if next.selected.as_ref() == Some(&old.key.word) && old.at(&next, "") {
                old.selected = true;
                self.previous = Some(old);
            } else if empty && old.at(&next, &old.remaining) {
                old.selected = false;
                self.previous = Some(old);
            } else if empty && old.at(&next, "") {
                update(path, None, Some(&old.key))?;
            } else if empty {
                // A sequence of backspaces may arrive separately or as one coalesced edit.
                let prefix = old
                    .remaining
                    .char_indices()
                    .skip(1)
                    .map(|(end, _)| &old.remaining[..end])
                    .find(|s| old.at(&next, s))
                    .map(str::to_owned);
                if let Some(prefix) = prefix {
                    old.remaining = prefix;
                    old.selected = false;
                    self.previous = Some(old);
                } else if old.selected
                    && old.before.right == next.right
                    && next
                        .left
                        .as_deref()
                        .zip(old.before.left.as_deref())
                        .is_some_and(|(left, before)| left.starts_with(before))
                {
                    update(path, None, Some(&old.key))?;
                }
            } else if next.selected == old.before.selected && old.at(&next, "") {
                update(path, None, Some(&old.key))?;
            }
        }
        self.context = Some(next);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(left: &str, selected: &str) -> Context {
        Context {
            left: Some(left.into()),
            right: Some("尾".into()),
            selected: Some(selected.into()),
            ..Default::default()
        }
    }
    fn key(word: &str) -> Choice {
        Choice {
            code: "ab".into(),
            word: word.into(),
        }
    }
    #[test]
    fn only_the_corrected_selection_is_withdrawn() {
        for correction in ["backspace", "undo", "replace"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("counts.json");
            let mut t = Tracker::default();
            t.observe(&path, state("前", "")).unwrap();
            for after in ["前以及", "前以及以及"] {
                t.committed(Some(key("以及")));
                t.observe(&path, state(after, "")).unwrap();
            }
            match correction {
                "backspace" => {
                    t.observe(&path, state("前以及以", "")).unwrap();
                    t.observe(&path, state("前以及", "")).unwrap();
                }
                "undo" => t.observe(&path, state("前以及", "")).unwrap(),
                _ => t.observe(&path, state("前以及", "以及")).unwrap(),
            }
            t.committed(Some(key("已经")));
            t.observe(&path, state("前以及已经", "")).unwrap();
            t.observe(&path, state("前以及已经", "")).unwrap();
            let data = counts(&path).unwrap();
            assert_eq!(data["ab"]["以及"], 1, "{correction}");
            assert_eq!(data["ab"]["已经"], 1, "{correction}");
        }
    }
    #[test]
    fn two_hosts_keep_independent_corrections_and_one_total() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("counts.json");
        std::thread::scope(|scope| {
            for keep in [true, false] {
                let path = &path;
                scope.spawn(move || {
                    let mut t = Tracker::default();
                    t.observe(path, state("", "")).unwrap();
                    t.committed(Some(key("已经")));
                    t.observe(path, state("已经", "")).unwrap();
                    if !keep {
                        t.observe(path, state("", "")).unwrap();
                    }
                });
            }
        });
        assert_eq!(counts(&path).unwrap()["ab"]["已经"], 1);
    }
}
