//! Shared data validation and saving. UI drafts stay with the caller until a successful result.
use crate::{
    config::ValidationContext, Config, Entry, TableConfig, TemporaryConfig, UserDictionary,
};
use parking_lot::Mutex;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

// All managed writers in this process share the check/write sequence. External editors
// do not participate: this deliberately does not promise filesystem compare-and-swap.
static WRITES: Mutex<()> = Mutex::new(());

/// Write a unique sibling then replace one file. A failed write never truncates the old file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing file name")
    })?;
    let (temporary, mut file) = loop {
        let mut temp_name = name.to_os_string();
        temp_name.push(format!(
            ".{}-{}.tmp",
            std::process::id(),
            NEXT_WRITE.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = parent.join(temp_name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    };
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| std::fs::rename(&temporary, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Exact file bytes detect external edits, including same-size/same-timestamp edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersion(Option<Arc<[u8]>>);
impl FileVersion {
    pub fn read(path: &Path) -> Result<Self, DataError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Self(Some(bytes.into()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self(None)),
            Err(error) => Err(DataError::Failed(error.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub enum WritePolicy {
    LegacyOverwrite,
    Expected(FileVersion),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DataError {
    #[error("文件已由其他写入者修改: {0}")]
    Conflict(PathBuf),
    #[error("{0}")]
    Failed(String),
}

fn check(path: &Path, policy: &WritePolicy) -> Result<(), DataError> {
    if let WritePolicy::Expected(expected) = policy {
        if &FileVersion::read(path)? != expected {
            return Err(DataError::Conflict(path.to_owned()));
        }
    }
    Ok(())
}

/// Validate nonempty references even when the desktop temporary entry is disabled.
pub fn validate_references(config: &TableConfig, path: &Path) -> Result<(), String> {
    for (pinyin, reference) in [
        (true, &config.temporary.pinyin_table),
        (false, &config.temporary.english_table),
    ] {
        if !reference.is_empty() {
            TemporaryConfig::load_table(path, reference, pinyin)?;
        }
    }
    Ok(())
}

/// Preserve the original referenced resource when saving a table in another directory.
pub fn relocated_config(
    config: &TableConfig,
    original: &Path,
    target: &Path,
) -> Result<TableConfig, String> {
    let mut config = config.clone();
    if target.parent() != original.parent() {
        for reference in [
            &mut config.temporary.pinyin_table,
            &mut config.temporary.english_table,
        ] {
            if !reference.is_empty() && Path::new(reference).is_relative() {
                if let Some(path) = TemporaryConfig::resolve_path(original, reference) {
                    *reference = std::path::absolute(path)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .into_owned();
                }
            }
        }
    }
    validate_references(&config, target)?;
    Ok(config)
}

pub fn save_config(
    config: &Config,
    path: &Path,
    context: ValidationContext,
    policy: WritePolicy,
) -> Result<FileVersion, DataError> {
    let _guard = WRITES.lock();
    check(path, &policy)?;
    config
        .save_for(path, context)
        .map_err(|e| DataError::Failed(e.to_string()))?;
    FileVersion::read(path)
}

pub fn save_table(
    original: &Path,
    target: &Path,
    config: &TableConfig,
    entries: &[Entry],
    policy: WritePolicy,
) -> Result<(TableConfig, FileVersion), DataError> {
    let _guard = WRITES.lock();
    check(target, &policy)?;
    let config = relocated_config(config, original, target).map_err(DataError::Failed)?;
    crate::save_table(target, &config, entries).map_err(|e| DataError::Failed(e.to_string()))?;
    Ok((config, FileVersion::read(target)?))
}

/// Caller keeps the original draft; partial success explicitly lists completed files.
#[derive(Debug)]
pub struct SaveReport {
    pub saved: Vec<PathBuf>,
    pub failure: Option<(PathBuf, DataError)>,
}

pub struct TableDraft<'a> {
    pub original: &'a Path,
    pub target: &'a Path,
    pub config: &'a TableConfig,
    pub entries: &'a [Entry],
    pub policy: WritePolicy,
}

/// Validate everything before writing. This is a sequence of atomic files, not a transaction.
pub fn save_bundle(
    config: &Config,
    config_path: &Path,
    context: ValidationContext,
    config_policy: WritePolicy,
    table: Option<TableDraft<'_>>,
) -> SaveReport {
    let _guard = WRITES.lock();
    let failed = |path: &Path, error| SaveReport {
        saved: Vec::new(),
        failure: Some((path.to_owned(), error)),
    };
    if let Err(e) = config.validate_for(context) {
        return failed(config_path, DataError::Failed(e.to_string()));
    }
    if let Err(e) = check(config_path, &config_policy) {
        return failed(config_path, e);
    }
    let prepared = if let Some(table) = table {
        if let Err(e) = check(table.target, &table.policy) {
            return failed(table.target, e);
        }
        let header = match relocated_config(table.config, table.original, table.target) {
            Ok(header) => header,
            Err(e) => return failed(table.target, DataError::Failed(e)),
        };
        if context == ValidationContext::DesktopSave {
            if let Err(e) = crate::validate_temporary_hotkeys(&header, &config.hotkey) {
                return failed(table.target, DataError::Failed(e));
            }
        }
        let report = crate::validate_table(&header, table.entries, 1);
        if let Some(issue) = report.issues.first() {
            return failed(table.target, DataError::Failed(issue.message.clone()));
        }
        Some((table, header))
    } else {
        None
    };
    let mut report = SaveReport {
        saved: Vec::new(),
        failure: None,
    };
    if let Some((table, header)) = prepared {
        if let Err(e) = crate::save_table(table.target, &header, table.entries) {
            return failed(table.target, DataError::Failed(e.to_string()));
        }
        report.saved.push(table.target.to_owned());
    }
    match config.save_for(config_path, context) {
        Ok(()) => report.saved.push(config_path.to_owned()),
        Err(e) => report.failure = Some((config_path.to_owned(), DataError::Failed(e.to_string()))),
    }
    report
}

/// Stable identity is (code, word), never an index from a previous UI snapshot.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UserEntryId {
    pub code: String,
    pub word: String,
}
pub enum UserEdit {
    Add(Entry),
    Update(UserEntryId, Entry),
    Remove(UserEntryId),
    Import(PathBuf),
}
#[derive(Debug)]
pub struct UserEditResult {
    pub version: FileVersion,
    pub added: usize,
    pub updated: usize,
}

pub fn edit_user(
    path: &Path,
    policy: WritePolicy,
    edit: UserEdit,
) -> Result<UserEditResult, DataError> {
    let _guard = WRITES.lock();
    check(path, &policy)?;
    let mut user = UserDictionary::load(path).map_err(|e| DataError::Failed(e.to_string()))?;
    let before = user.entries().to_vec();
    let find = |user: &UserDictionary, id: &UserEntryId| {
        user.entries()
            .iter()
            .position(|e| e.code == id.code && e.word == id.word)
            .ok_or_else(|| DataError::Failed("词条已不存在".into()))
    };
    match edit {
        UserEdit::Add(entry) => user.add(entry),
        UserEdit::Update(id, entry) => {
            let index = find(&user, &id)?;
            user.update(index, entry)
        }
        UserEdit::Remove(id) => {
            let index = find(&user, &id)?;
            user.remove(index).map(|_| ())
        }
        UserEdit::Import(source) => user.import(source).map(|_| ()),
    }
    .map_err(|e| DataError::Failed(e.to_string()))?;
    let mut added = 0;
    let mut updated = 0;
    for entry in user.entries() {
        match before
            .iter()
            .find(|e| e.code == entry.code && e.word == entry.word)
        {
            None => added += 1,
            Some(old) if old.weight != entry.weight => updated += 1,
            _ => {}
        }
    }
    Ok(UserEditResult {
        version: FileVersion::read(path)?,
        added,
        updated,
    })
}

pub fn export_user(
    source: &Path,
    target: &Path,
    policy: WritePolicy,
) -> Result<FileVersion, DataError> {
    let _guard = WRITES.lock();
    check(target, &policy)?;
    UserDictionary::load(source)
        .and_then(|u| u.export(target))
        .map_err(|e| DataError::Failed(e.to_string()))?;
    FileVersion::read(target)
}

/// Validate an explicitly chosen main file and its direct references inside an import root.
/// Absolute foreign references require an explicit rebinding before this check.
pub fn validate_package(root: &Path, main: &Path) -> Result<Vec<PathBuf>, DataError> {
    let canonical = |p: &Path| {
        p.canonicalize()
            .map_err(|e| DataError::Failed(e.to_string()))
    };
    let root = canonical(root)?;
    let main = canonical(main)?;
    if !main.starts_with(&root) {
        return Err(DataError::Failed("主表越出导入目录".into()));
    }
    let dict = crate::Dictionary::load(&main).map_err(|e| DataError::Failed(e.to_string()))?;
    let mut paths = vec![main.clone()];
    for reference in [
        &dict.table_config().temporary.pinyin_table,
        &dict.table_config().temporary.english_table,
    ] {
        if reference.is_empty() {
            continue;
        }
        if Path::new(reference).is_absolute()
            || reference.contains(':')
            || reference.starts_with('\\')
        {
            return Err(DataError::Failed("绝对引用需要显式重绑定".into()));
        }
        let path = canonical(&main.parent().unwrap_or(&root).join(reference))?;
        if !path.starts_with(&root) {
            return Err(DataError::Failed("辅助引用越出导入目录".into()));
        }
        paths.push(path);
    }
    validate_references(dict.table_config(), &main).map_err(DataError::Failed)?;
    Ok(paths)
}
