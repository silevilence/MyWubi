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

#[derive(Default, Serialize, Deserialize)]
pub struct Statistics {
    // A store identity distinguishes independent devices when a user dictionary is imported.
    pub id: String,
    pub counts: Counts,
    scales: BTreeMap<String, i64>,
}

pub fn statistics(path: &Path) -> io::Result<Statistics> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Statistics::default()),
        Err(e) => Err(e),
    }
}

pub(crate) type Scores = BTreeMap<String, BTreeMap<String, i64>>;

impl Statistics {
    pub(crate) fn scores(&self, saved: &Counts) -> Scores {
        let mut scores = Scores::new();
        for (code, words) in &self.counts {
            let scale = self.scales.get(code).copied().unwrap_or(1);
            for (word, count) in words {
                let saved = saved
                    .get(code)
                    .and_then(|row| row.get(word))
                    .copied()
                    .unwrap_or(0);
                let fresh = count.saturating_sub(saved);
                if fresh > 0 {
                    let score = i64::try_from(fresh)
                        .unwrap_or(i64::MAX)
                        .saturating_mul(scale);
                    scores
                        .entry(code.clone())
                        .or_default()
                        .insert(word.clone(), score);
                }
            }
        }
        scores
    }
}

#[derive(Default)]
pub(crate) struct Ranking {
    pub scores: Scores,
    pub ties: BTreeMap<String, Vec<String>>,
}
impl Ranking {
    pub fn weight(&self, row: &crate::Entry) -> i64 {
        let extra = self
            .scores
            .get(&row.code)
            .and_then(|words| words.get(&row.word))
            .copied()
            .unwrap_or(0);
        row.weight.saturating_add(extra)
    }

    pub fn sort(&self, code: &str, rows: &mut [&crate::Entry]) {
        if self.scores.is_empty() {
            return;
        }
        rows.sort_by(|a, b| {
            (b.code == code)
                .cmp(&(a.code == code))
                .then_with(|| self.weight(b).cmp(&self.weight(a)))
                .then_with(|| a.code.cmp(&b.code))
                .then_with(|| {
                    let Some(ties) = self.ties.get(&a.code).filter(|_| a.code == b.code) else {
                        return std::cmp::Ordering::Equal;
                    };
                    let pos = |row: &crate::Entry| {
                        ties.iter()
                            .position(|w| *w == row.word)
                            .unwrap_or(usize::MAX)
                    };
                    pos(a).cmp(&pos(b))
                })
        });
    }

    pub fn load(runtime: &crate::runtime::RuntimeSnapshot) -> Result<Self, String> {
        let path =
            store_path(&runtime.config_path, &runtime.system_table_path).ok_or("词频目录不可用")?;
        let data = statistics(&path).map_err(|e| e.to_string())?;
        let mut ranking = Self {
            scores: data.scores(&Counts::new()),
            ..Default::default()
        };
        if runtime.config.dictionary.enable_user_dict {
            let user =
                crate::UserDictionary::load(&runtime.user_table_path).map_err(|e| e.to_string())?;
            if let Some(saved) = user.learned.get(&data.id) {
                ranking.scores = data.scores(saved);
            }
            ranking.ties = user.order;
        }
        Ok(ranking)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Preview {
    pub count: usize,
    pub examples: Vec<String>,
    pub version: String,
}

/// The user's adjustment and the consumed counts share one atomic file replacement.
pub fn materialize(config_path: &Path, confirm: Option<&str>) -> Result<Preview, String> {
    use crate::{config_path::resolve_resource_path, Config, Dictionary, Entry, UserDictionary};
    let config = Config::load(config_path).map_err(|e| e.to_string())?;
    if !config.dictionary.enable_user_dict {
        return Err("请先启用用户词库并保存设置".into());
    }
    let table = resolve_resource_path(config_path, &config.dictionary.system_table);
    let path = resolve_resource_path(config_path, &config.dictionary.user_table);
    let file = store_path(config_path, &table).ok_or("词频目录不可用")?;
    let _lock = if confirm.is_some() {
        Some(lock_file(&file).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let data = statistics(&file).map_err(|e| e.to_string())?;
    let mut user = UserDictionary::for_edit(&path, Some(&table)).map_err(|e| e.to_string())?;
    let system = Dictionary::load(&table).map_err(|e| e.to_string())?;
    let dict = crate::runtime::merge_dictionary(system, &path, &config)?;
    let empty = Counts::new();
    let scores = data.scores(user.learned.get(&data.id).unwrap_or(&empty));
    let ranking = Ranking {
        scores,
        ties: user.order.clone(),
    };
    let mut entries: BTreeMap<_, _> = user
        .entries()
        .iter()
        .map(|e| ((e.code.clone(), e.word.clone()), e.weight))
        .collect();
    let mut order = user.order.clone();
    let mut learned = user.learned.clone();
    let mut examples = Vec::new();
    let mut count = 0;
    for (code, words) in &ranking.scores {
        let mut rows = dict.exact(code);
        let mut seen = std::collections::HashSet::new();
        rows.retain(|e| seen.insert(e.word.as_str()));
        let before: BTreeMap<_, _> = rows
            .iter()
            .enumerate()
            .map(|(i, e)| (e.word.as_str(), i + 1))
            .collect();
        ranking.sort(code, &mut rows);
        for (index, row) in rows.iter().enumerate() {
            let Some(score) = words.get(&row.word) else {
                continue;
            };
            let value = entries.entry((code.clone(), row.word.clone())).or_default();
            *value = value.checked_add(*score).ok_or("词频调整值超出范围")?;
            learned
                .entry(data.id.clone())
                .or_default()
                .entry(code.clone())
                .or_default()
                .insert(row.word.clone(), data.counts[code][&row.word]);
            count += 1;
            if examples.len() < 5 {
                examples.push(format!(
                    "{code}  {}：第 {} 位 → 第 {} 位（+{score}）",
                    row.word,
                    before[row.word.as_str()],
                    index + 1
                ));
            }
        }
        if !rows.is_empty() {
            order.insert(
                code.clone(),
                rows.into_iter().map(|e| e.word.clone()).collect(),
            );
        }
    }
    let entries: Vec<_> = entries
        .into_iter()
        .map(|((code, word), weight)| Entry { code, word, weight })
        .collect();
    let preview_bytes = serde_json::to_vec(&(&path, &table, &entries, &order, &learned, &examples))
        .map_err(|e| e.to_string())?;
    let preview = Preview {
        count,
        examples,
        version: format!("{:x}", Sha256::digest(preview_bytes)),
    };
    if let Some(expected) = confirm {
        if preview.count == 0 {
            return Ok(preview);
        }
        if expected != preview.version {
            return Err("词频或词典已变化，请重新预览后确认".into());
        }
        user.replace_all(entries, order, learned)
            .map_err(|e| e.to_string())?;
    }
    Ok(preview)
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
    pub scale: i64,
}

fn update(path: &Path, add: Option<&Choice>, remove: Option<&Choice>) -> io::Result<()> {
    if add == remove {
        return Ok(());
    }
    let _lock = lock_file(path)?;
    let mut data = statistics(path)?;
    if data.id.is_empty() {
        use std::hash::{BuildHasher, RandomState};
        data.id = format!(
            "{:016x}{:016x}",
            RandomState::new().hash_one(0),
            RandomState::new().hash_one(1)
        );
    }
    let totals = &mut data.counts;
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
        data.scales.entry(key.code.clone()).or_insert(key.scale);
        let n = totals
            .entry(key.code.clone())
            .or_default()
            .entry(key.word.clone())
            .or_default();
        *n = n.saturating_add(1);
    }
    crate::data::atomic_write(path, &serde_json::to_vec(&data)?)
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
    pub changed: bool,
}

impl Tracker {
    pub fn committed(&mut self, key: Option<Choice>) {
        self.waiting = key
            .zip(self.context.clone())
            .map(|(key, before)| Selection::new(key, before));
    }

    pub fn observe(&mut self, path: &Path, next: Context) -> io::Result<()> {
        self.changed = false;
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
                self.changed = true;
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
                self.changed = true;
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
                    self.changed = true;
                }
            } else if next.selected == old.before.selected && old.at(&next, "") {
                update(path, None, Some(&old.key))?;
                self.changed = true;
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
            scale: 1,
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
            let data = statistics(&path).unwrap().counts;
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
        assert_eq!(statistics(&path).unwrap().counts["ab"]["已经"], 1);
    }
}

#[cfg(test)]
mod ranking_tests {
    use super::*;
    #[test]
    fn recording_and_ranking_can_be_enabled_independently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frequency.json");
        let key = Choice {
            code: "ab".into(),
            word: "乙".into(),
            scale: 4,
        };
        for _ in 0..3 {
            update(&path, Some(&key), None).unwrap();
        }
        let counts = statistics(&path).unwrap();
        let ranking = Ranking {
            scores: counts.scores(&Counts::new()),
            ..Default::default()
        };
        let dictionary = crate::Dictionary::from_text("ab\t甲\t20\nab\t乙\t10\n").unwrap();
        let mut rows = dictionary.exact("ab");
        ranking.sort("ab", &mut rows);
        assert_eq!(rows[0].word, "乙");
        assert_eq!(dictionary.exact("ab")[0].word, "甲");
        update(&path, None, Some(&key)).unwrap();
        let ranking = Ranking {
            scores: statistics(&path).unwrap().scores(&Counts::new()),
            ..Default::default()
        };
        ranking.sort("ab", &mut rows);
        assert_eq!(rows[0].word, "甲");
    }
}

#[cfg(test)]
mod materialize_tests {
    use super::*;
    use crate::{Config, UserDictionary};

    #[test]
    fn confirmed_batch_consumes_counts_atomically_and_rejects_changed_preview() {
        let private = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
        let root = tempfile::tempdir_in(private).unwrap();
        let config = root.path().join("config.toml");
        let table = root.path().join("main.dict");
        let user = root.path().join("user.dict");
        fs::write(&table, "ab\t甲\t20\nab\t乙\t10\n").unwrap();
        fs::write(&user, "# mywubi-user-dict version=2\nab\t乙\t5\n").unwrap();
        let mut c = Config::default();
        c.dictionary.enable_user_dict = true;
        c.dictionary.system_table = table.clone();
        c.dictionary.user_table = user.clone();
        c.save(&config).unwrap();
        let file = store_path(&config, &table).unwrap();
        let key = Choice {
            code: "ab".into(),
            word: "乙".into(),
            scale: 4,
        };
        update(&file, Some(&key), None).unwrap();
        let old = materialize(&config, None).unwrap();
        update(&file, Some(&key), None).unwrap();
        update(&file, Some(&key), None).unwrap();
        assert!(materialize(&config, Some(&old.version)).is_err());
        assert_eq!(UserDictionary::load(&user).unwrap().entries()[0].weight, 5);
        let preview = materialize(&config, None).unwrap();
        assert_eq!(preview.count, 1);
        let mut stale_editor = UserDictionary::for_edit(&user, Some(&table)).unwrap();
        assert_eq!(
            materialize(&config, Some(&preview.version)).unwrap().count,
            1
        );
        assert!(stale_editor.remove(0).is_err());
        assert_eq!(UserDictionary::load(&user).unwrap().entries()[0].weight, 17);
        assert_eq!(
            materialize(&config, Some(&preview.version)).unwrap().count,
            0
        );
        assert_eq!(materialize(&config, None).unwrap().count, 0);
        assert_eq!(
            fs::read_to_string(&table).unwrap(),
            "ab\t甲\t20\nab\t乙\t10\n"
        );
    }
}
