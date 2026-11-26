//! 独立于 `config.toml` 的用户词库持久化与 C-ABI。

use std::collections::{BTreeMap, HashSet};
use std::ffi::{c_char, CStr, CString};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::dictionary::Entry;

const HEADER: &str = "# mywubi-user-dict version=2";
const LEGACY_HEADER: &str = "# mywubi-user-dict version=1";

#[derive(Debug, Error)]
pub enum UserDictionaryError {
    #[error("无法读写用户词库 {0}: {1}")]
    Io(PathBuf, String),
    #[error("用户词库第 {0} 行格式非法: {1}")]
    InvalidLine(usize, String),
    #[error("词条非法: {0}")]
    InvalidEntry(String),
    #[error("词条已存在: {0} / {1}")]
    Duplicate(String, String),
    #[error("词条索引越界: {0}")]
    Index(usize),
}

#[derive(Debug)]
pub struct UserDictionary {
    path: PathBuf,
    additive: bool,
    system_path: Option<PathBuf>,
    entries: Vec<Entry>,
    pub(crate) learned: BTreeMap<String, crate::frequency::Counts>,
    baseline: Option<[u8; 32]>,
    pub(crate) order: BTreeMap<String, Vec<String>>,
}

impl UserDictionary {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, UserDictionaryError> {
        let path = path.as_ref().to_path_buf();
        let (entries, additive, order, learned, baseline) = match std::fs::read_to_string(&path) {
            Ok(text) => (
                parse(&text)?,
                text.lines().any(|line| line.trim() == HEADER),
                read_order(&text)?,
                read_learned(&text)?,
                Some(fingerprint(text.as_bytes())),
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Vec::new(), true, BTreeMap::new(), BTreeMap::new(), None)
            }
            Err(error) => return Err(UserDictionaryError::Io(path, error.to_string())),
        };
        Ok(Self {
            path,
            additive,
            system_path: None,
            order,
            entries,
            learned,
            baseline,
        })
    }

    /// Present legacy absolute weights as adjustments. Opening alone leaves the file intact.
    pub fn for_edit(path: &Path, system: Option<&Path>) -> Result<Self, UserDictionaryError> {
        let mut user = Self::load(path)?;
        user.system_path = system.map(Path::to_owned);
        if let Some(system) = system.filter(|_| !user.additive) {
            let base = crate::Dictionary::load(system)
                .map_err(|e| UserDictionaryError::Io(system.into(), e.to_string()))?;
            for entry in &mut user.entries {
                entry.weight -= base.base_weight(&entry.code, &entry.word);
            }
            user.additive = true;
        }
        Ok(user)
    }

    /// Store only the selected word's adjustment. Equal-score neighbours retain their order.
    pub fn move_to_rank(
        &mut self,
        visible: &crate::Dictionary,
        code: &str,
        word: &str,
        rank: usize,
    ) -> Result<(), UserDictionaryError> {
        let invalid = |message: &str| UserDictionaryError::InvalidEntry(message.into());
        if !self.additive {
            return Err(invalid("请先按当前码表载入用户词典"));
        }
        let mut seen = HashSet::new();
        let mut rows: Vec<_> = visible
            .exact(code)
            .into_iter()
            .filter(|row| seen.insert(row.word.as_str()))
            .collect();
        if rank == 0 || rank > rows.len() {
            return Err(invalid("指定名次超出同码候选数"));
        }
        let from = rows
            .iter()
            .position(|row| row.word == word)
            .ok_or_else(|| invalid("当前编码没有该候选"))?;
        if from == rank - 1 {
            return Ok(());
        }
        let destination = rows[rank - 1].weight;
        let entry = rows.remove(from);
        rows.insert(rank - 1, entry);
        let mut entries = self.entries.clone();
        let position = entries
            .iter()
            .position(|row| row.code == code && row.word == word);
        let previous = position.map_or(0, |i| entries[i].weight);
        let weight = i64::try_from(
            i128::from(previous) + i128::from(destination) - i128::from(entry.weight),
        )
        .map_err(|_| invalid("调整值超出范围"))?;
        if let Some(i) = position {
            entries[i].weight = weight;
        } else {
            entries.push(Entry {
                code: code.into(),
                word: word.into(),
                weight,
            });
        }
        let mut order = self.order.clone();
        order.insert(
            code.into(),
            rows.iter().map(|row| row.word.clone()).collect(),
        );
        self.replace_ordered(entries, order)
    }

    /// Stable input to the dictionary's weight sort, used only to break ties.
    pub(crate) fn order_ties(&self, entries: &mut [Entry]) {
        if self.order.is_empty() {
            return;
        }
        let ranks: std::collections::HashMap<_, _> = self
            .order
            .iter()
            .flat_map(|(code, words)| {
                words
                    .iter()
                    .enumerate()
                    .map(move |(i, word)| ((code.as_str(), word.as_str()), i))
            })
            .collect();
        entries.sort_by_key(|row| {
            ranks
                .get(&(row.code.as_str(), row.word.as_str()))
                .copied()
                .unwrap_or(usize::MAX)
        });
    }

    pub fn additive(&self) -> bool {
        self.additive
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn add(&mut self, entry: Entry) -> Result<(), UserDictionaryError> {
        validate(&entry)?;
        if self
            .entries
            .iter()
            .any(|item| item.code == entry.code && item.word == entry.word)
        {
            return Err(UserDictionaryError::Duplicate(entry.code, entry.word));
        }
        let mut entries = self.entries.clone();
        entries.push(entry);
        self.replace_entries(entries)
    }

    pub fn update(&mut self, index: usize, entry: Entry) -> Result<(), UserDictionaryError> {
        validate(&entry)?;
        if index >= self.entries.len() {
            return Err(UserDictionaryError::Index(index));
        }
        if self.entries.iter().enumerate().any(|(other, item)| {
            other != index && item.code == entry.code && item.word == entry.word
        }) {
            return Err(UserDictionaryError::Duplicate(entry.code, entry.word));
        }
        let mut entries = self.entries.clone();
        entries[index] = entry;
        self.replace_entries(entries)
    }

    pub fn remove(&mut self, index: usize) -> Result<Entry, UserDictionaryError> {
        if index >= self.entries.len() {
            return Err(UserDictionaryError::Index(index));
        }
        let mut entries = self.entries.clone();
        let removed = entries.remove(index);
        self.replace_entries(entries)?;
        Ok(removed)
    }

    pub fn import(&mut self, path: impl AsRef<Path>) -> Result<usize, UserDictionaryError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| UserDictionaryError::Io(path.to_path_buf(), error.to_string()))?;
        let mut imported = parse(&text)?;
        if self.additive && !text.lines().any(|line| line.trim() == HEADER) {
            if let Some(path) = &self.system_path {
                let base = crate::Dictionary::load(path)
                    .map_err(|e| UserDictionaryError::Io(path.clone(), e.to_string()))?;
                for entry in &mut imported {
                    entry.weight -= base.base_weight(&entry.code, &entry.word);
                }
            }
        }
        let before = self.entries.len();
        let mut entries = self.entries.clone();
        for entry in imported {
            if let Some(existing) = entries
                .iter_mut()
                .find(|item| item.code == entry.code && item.word == entry.word)
            {
                existing.weight = entry.weight;
            } else {
                entries.push(entry);
            }
        }
        let added = entries.len() - before;
        let mut order = self.order.clone();
        order.extend(read_order(&text)?);
        let mut learned = self.learned.clone();
        for (id, counts) in read_learned(&text)? {
            for (code, words) in counts {
                learned
                    .entry(id.clone())
                    .or_default()
                    .entry(code)
                    .or_default()
                    .extend(words);
            }
        }
        self.replace_all(entries, order, learned)?;
        Ok(added)
    }

    pub fn export(&self, path: impl AsRef<Path>) -> Result<(), UserDictionaryError> {
        write_entries(
            path.as_ref(),
            &self.entries,
            self.additive,
            &self.order,
            &self.learned,
        )
        .map(|_| ())
    }

    pub fn save(&mut self) -> Result<(), UserDictionaryError> {
        self.replace_entries(self.entries.clone())
    }

    fn replace_entries(&mut self, entries: Vec<Entry>) -> Result<(), UserDictionaryError> {
        self.replace_ordered(entries, self.order.clone())
    }

    fn replace_ordered(
        &mut self,
        entries: Vec<Entry>,
        order: BTreeMap<String, Vec<String>>,
    ) -> Result<(), UserDictionaryError> {
        self.replace_all(entries, order, self.learned.clone())
    }

    pub(crate) fn replace_all(
        &mut self,
        mut entries: Vec<Entry>,
        order: BTreeMap<String, Vec<String>>,
        learned: BTreeMap<String, crate::frequency::Counts>,
    ) -> Result<(), UserDictionaryError> {
        let error = |e: std::io::Error| UserDictionaryError::Io(self.path.clone(), e.to_string());
        let _guard = crate::frequency::lock_file(&self.path).map_err(error)?;
        let current = match std::fs::read(&self.path) {
            Ok(bytes) => Some(fingerprint(&bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(error(e)),
        };
        if self.baseline != current {
            return Err(UserDictionaryError::InvalidEntry(
                "用户词典已修改，请重新载入".into(),
            ));
        }
        for entry in &entries {
            validate(entry)?;
        }
        entries.sort_by(|left, right| {
            left.code
                .cmp(&right.code)
                .then_with(|| right.weight.cmp(&left.weight))
                .then_with(|| left.word.cmp(&right.word))
        });
        self.baseline = Some(write_entries(
            &self.path,
            &entries,
            self.additive,
            &order,
            &learned,
        )?);
        self.learned = learned;
        self.order = order;
        self.entries = entries;
        Ok(())
    }
}

fn fingerprint(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).into()
}

const LEARNED_PREFIX: &str = "# mywubi-learned ";

fn read_learned(
    text: &str,
) -> Result<BTreeMap<String, crate::frequency::Counts>, UserDictionaryError> {
    for (i, line) in text.lines().enumerate() {
        if let Some(json) = line.strip_prefix(LEARNED_PREFIX) {
            return serde_json::from_str(json)
                .map_err(|e| UserDictionaryError::InvalidLine(i + 1, e.to_string()));
        }
    }
    Ok(BTreeMap::new())
}

const ORDER_PREFIX: &str = "# mywubi-order ";

fn read_order(text: &str) -> Result<BTreeMap<String, Vec<String>>, UserDictionaryError> {
    let mut order = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        if let Some(json) = line.strip_prefix(ORDER_PREFIX) {
            let (code, words) = serde_json::from_str(json).map_err(|e| {
                UserDictionaryError::InvalidLine(n + 1, format!("同分顺序无效: {e}"))
            })?;
            order.insert(code, words);
        }
    }
    Ok(order)
}

fn validate(entry: &Entry) -> Result<(), UserDictionaryError> {
    if entry.code.is_empty() || !entry.code.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(UserDictionaryError::InvalidEntry(
            "编码只能包含 ASCII 字母或数字".into(),
        ));
    }
    if entry.word.is_empty()
        || entry
            .word
            .chars()
            .any(|ch| matches!(ch, '\0' | '\t' | '\r' | '\n'))
    {
        return Err(UserDictionaryError::InvalidEntry(
            "词条不能为空或包含制表/换行字符".into(),
        ));
    }
    Ok(())
}

fn parse(text: &str) -> Result<Vec<Entry>, UserDictionaryError> {
    let additive = text.lines().any(|line| line.trim() == HEADER);
    let mut entries = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let entry = Entry {
            code: fields.next().unwrap_or_default().trim().to_string(),
            word: fields.next().unwrap_or_default().trim().to_string(),
            weight: fields.next().unwrap_or("1").trim().parse().map_err(|_| {
                UserDictionaryError::InvalidLine(index + 1, "词频必须是有符号整数".into())
            })?,
        };
        if !additive && u32::try_from(entry.weight).is_err() {
            return Err(UserDictionaryError::InvalidLine(
                index + 1,
                "旧版词频超出范围".into(),
            ));
        }
        validate(&entry)
            .map_err(|error| UserDictionaryError::InvalidLine(index + 1, error.to_string()))?;
        entries.push(entry);
    }
    entries.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| right.weight.cmp(&left.weight))
            .then_with(|| left.word.cmp(&right.word))
    });
    entries.dedup_by(|left, right| left.code == right.code && left.word == right.word);
    Ok(entries)
}

fn write_entries(
    path: &Path,
    entries: &[Entry],
    additive: bool,
    order: &BTreeMap<String, Vec<String>>,
    learned: &BTreeMap<String, crate::frequency::Counts>,
) -> Result<[u8; 32], UserDictionaryError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| UserDictionaryError::Io(parent.to_path_buf(), error.to_string()))?;
    }
    let header = if additive { HEADER } else { LEGACY_HEADER };
    let mut text = format!("{header}\n");
    if !learned.is_empty() {
        let json = serde_json::to_string(learned)
            .map_err(|e| UserDictionaryError::InvalidEntry(e.to_string()))?;
        text.push_str(&format!("{LEARNED_PREFIX}{json}\n"));
    }
    for row in order {
        let json = serde_json::to_string(&row)
            .map_err(|e| UserDictionaryError::InvalidEntry(e.to_string()))?;
        text.push_str(&format!("{ORDER_PREFIX}{json}\n"));
    }
    for entry in entries {
        text.push_str(&format!(
            "{}\t{}\t{}\n",
            entry.code, entry.word, entry.weight
        ));
    }
    crate::data::atomic_write(path, text.as_bytes())
        .map(|_| fingerprint(text.as_bytes()))
        .map_err(|error| UserDictionaryError::Io(path.to_path_buf(), error.to_string()))
}

#[repr(C)]
pub struct UserDictionaryEntry {
    pub code: *mut c_char,
    pub word: *mut c_char,
    pub weight: u32,
}

unsafe fn ffi_text(pointer: *const c_char) -> Result<String, UserDictionaryError> {
    if pointer.is_null() {
        return Err(UserDictionaryError::InvalidEntry("字符串指针为空".into()));
    }
    // SAFETY: FFI 调用方保证 pointer 指向有效的 NUL 结尾字符串。
    let text = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map_err(|error| UserDictionaryError::InvalidEntry(error.to_string()))?;
    Ok(text.to_owned())
}

fn ffi_status(operation: impl FnOnce() -> Result<(), UserDictionaryError>) -> i32 {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)) {
        Ok(Ok(())) => 0,
        Ok(Err(_)) => -1,
        Err(_) => -2,
    }
}

/// 打开或创建用户词库句柄。
///
/// # Safety
///
/// `path` 必须指向有效的 UTF-8、NUL 结尾字符串。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_open(path: *const c_char) -> *mut UserDictionary {
    std::panic::catch_unwind(|| {
        // SAFETY: 由本函数的调用契约保证。
        let path = unsafe { ffi_text(path) }.ok()?;
        UserDictionary::load(path)
            .ok()
            .map(|dictionary| Box::into_raw(Box::new(dictionary)))
    })
    .ok()
    .flatten()
    .unwrap_or(std::ptr::null_mut())
}

/// 释放用户词库句柄。
///
/// # Safety
///
/// `dictionary` 必须为空，或由 [`mywubi_user_dict_open`] 返回且尚未释放。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_destroy(dictionary: *mut UserDictionary) {
    if !dictionary.is_null() {
        // SAFETY: pointer 必须由 mywubi_user_dict_open 返回且只释放一次。
        drop(unsafe { Box::from_raw(dictionary) });
    }
}

/// 返回用户词库词条数。
///
/// # Safety
///
/// `dictionary` 必须为空，或指向调用期间有效的用户词库句柄。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_len(dictionary: *const UserDictionary) -> usize {
    // SAFETY: FFI 调用方保证 handle 在调用期间有效。
    unsafe { dictionary.as_ref() }
        .map(|dictionary| dictionary.entries.len())
        .unwrap_or(0)
}

/// 复制指定词条供 C 调用方读取。
///
/// # Safety
///
/// `dictionary` 必须为空，或指向调用期间有效的用户词库句柄。返回值须交给
/// [`mywubi_user_dict_entry_destroy`] 释放。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_entry_at(
    dictionary: *const UserDictionary,
    index: usize,
) -> *mut UserDictionaryEntry {
    std::panic::catch_unwind(|| {
        // SAFETY: FFI 调用方保证 handle 在调用期间有效。
        let entry = unsafe { dictionary.as_ref() }?.entries.get(index)?;
        let weight = u32::try_from(entry.weight).ok()?;
        let code = CString::new(entry.code.as_str()).ok()?.into_raw();
        let word = match CString::new(entry.word.as_str()) {
            Ok(word) => word.into_raw(),
            Err(_) => {
                // SAFETY: code 刚由 CString::into_raw 创建。
                drop(unsafe { CString::from_raw(code) });
                return None;
            }
        };
        Some(Box::into_raw(Box::new(UserDictionaryEntry {
            code,
            word,
            weight,
        })))
    })
    .ok()
    .flatten()
    .unwrap_or(std::ptr::null_mut())
}

/// 释放由 [`mywubi_user_dict_entry_at`] 返回的词条。
///
/// # Safety
///
/// `entry` 必须为空，或由 [`mywubi_user_dict_entry_at`] 返回且尚未释放。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_entry_destroy(entry: *mut UserDictionaryEntry) {
    if entry.is_null() {
        return;
    }
    // SAFETY: pointer 必须由 mywubi_user_dict_entry_at 返回且只释放一次。
    let entry = unsafe { Box::from_raw(entry) };
    if !entry.code.is_null() {
        // SAFETY: code 由 CString::into_raw 创建。
        drop(unsafe { CString::from_raw(entry.code) });
    }
    if !entry.word.is_null() {
        // SAFETY: word 由 CString::into_raw 创建。
        drop(unsafe { CString::from_raw(entry.word) });
    }
}

/// 新增并持久化词条。
///
/// # Safety
///
/// `dictionary` 必须是有效句柄，`code` 与 `word` 必须是有效的 UTF-8、
/// NUL 结尾字符串。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_add(
    dictionary: *mut UserDictionary,
    code: *const c_char,
    word: *const c_char,
    weight: u32,
) -> i32 {
    ffi_status(|| {
        // SAFETY: 由本函数的调用契约保证。
        let dictionary = unsafe { dictionary.as_mut() }
            .ok_or_else(|| UserDictionaryError::InvalidEntry("词库句柄为空".into()))?;
        dictionary.add(Entry {
            code: unsafe { ffi_text(code) }?,
            word: unsafe { ffi_text(word) }?,
            weight: i64::from(weight),
        })
    })
}

/// 更新并持久化指定索引的词条。
///
/// # Safety
///
/// `dictionary` 必须是有效句柄，`code` 与 `word` 必须是有效的 UTF-8、
/// NUL 结尾字符串。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_update(
    dictionary: *mut UserDictionary,
    index: usize,
    code: *const c_char,
    word: *const c_char,
    weight: u32,
) -> i32 {
    ffi_status(|| {
        // SAFETY: 由本函数的调用契约保证。
        let dictionary = unsafe { dictionary.as_mut() }
            .ok_or_else(|| UserDictionaryError::InvalidEntry("词库句柄为空".into()))?;
        dictionary.update(
            index,
            Entry {
                code: unsafe { ffi_text(code) }?,
                word: unsafe { ffi_text(word) }?,
                weight: i64::from(weight),
            },
        )
    })
}

/// 删除并持久化指定索引的词条。
///
/// # Safety
///
/// `dictionary` 必须指向调用期间有效的用户词库句柄。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_remove(
    dictionary: *mut UserDictionary,
    index: usize,
) -> i32 {
    ffi_status(|| {
        // SAFETY: 由本函数的调用契约保证。
        unsafe { dictionary.as_mut() }
            .ok_or_else(|| UserDictionaryError::InvalidEntry("词库句柄为空".into()))?
            .remove(index)
            .map(|_| ())
    })
}

/// 从另一份用户词库合并导入。
///
/// # Safety
///
/// `dictionary` 必须是有效句柄，`path` 必须是有效的 UTF-8、NUL 结尾字符串。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_import(
    dictionary: *mut UserDictionary,
    path: *const c_char,
) -> i32 {
    ffi_status(|| {
        // SAFETY: 由本函数的调用契约保证。
        unsafe { dictionary.as_mut() }
            .ok_or_else(|| UserDictionaryError::InvalidEntry("词库句柄为空".into()))?
            .import(unsafe { ffi_text(path) }?)
            .map(|_| ())
    })
}

/// 将当前用户词库导出到指定路径。
///
/// # Safety
///
/// `dictionary` 必须是有效句柄，`path` 必须是有效的 UTF-8、NUL 结尾字符串。
#[no_mangle]
pub unsafe extern "C" fn mywubi_user_dict_export(
    dictionary: *const UserDictionary,
    path: *const c_char,
) -> i32 {
    ffi_status(|| {
        // SAFETY: 由本函数的调用契约保证。
        unsafe { dictionary.as_ref() }
            .ok_or_else(|| UserDictionaryError::InvalidEntry("词库句柄为空".into()))?
            .export(unsafe { ffi_text(path) }?)
    })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mywubi-{name}-{}.dict",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn crud_roundtrip_persists_entries() {
        let path = temporary_path("user-dict");
        let mut dictionary = UserDictionary::load(&path).unwrap();
        dictionary
            .add(Entry {
                code: "abcd".into(),
                word: "测试".into(),
                weight: 100,
            })
            .unwrap();

        let loaded = UserDictionary::load(&path).unwrap();

        assert_eq!(loaded.entries()[0].word, "测试");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn import_merges_and_updates_duplicates() {
        let path = temporary_path("user-dict-target");
        let import_path = temporary_path("user-dict-import");
        std::fs::write(&import_path, "abcd\t测试\t200\nefgh\t词条\t50\n").unwrap();
        let mut dictionary = UserDictionary::load(&path).unwrap();
        dictionary
            .add(Entry {
                code: "abcd".into(),
                word: "测试".into(),
                weight: 1,
            })
            .unwrap();

        dictionary.import(&import_path).unwrap();

        assert_eq!(dictionary.entries()[0].weight, 200);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(import_path);
    }

    #[test]
    fn failed_save_keeps_in_memory_entries_unchanged() {
        let root = temporary_path("user-dict-blocked");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("user.dict");
        let mut dictionary = UserDictionary::load(&path).unwrap();
        dictionary
            .add(Entry {
                code: "abcd".into(),
                word: "保留".into(),
                weight: 1,
            })
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&root).unwrap();
        std::fs::write(&root, "blocked").unwrap();

        let result = dictionary.add(Entry {
            code: "efgh".into(),
            word: "不应保留".into(),
            weight: 1,
        });

        assert!(result.is_err() && dictionary.entries().len() == 1);
        let _ = std::fs::remove_file(root);
    }
}
