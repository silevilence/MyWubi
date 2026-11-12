//! Shared settings-side resource inspection and durable imports. No input-path I/O.
use crate::expressions::{
    ExpressionKind, ExpressionLibrary, ExpressionSelection, ExpressionSource,
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize)]
pub struct ResourceInfo {
    pub path: PathBuf,
    pub builtin: bool,
    pub name: String,
    pub version: String,
    pub entries: usize,
    pub sources: Vec<ExpressionSource>,
    pub error: Option<String>,
}

fn slug(kind: ExpressionKind) -> &'static str {
    kind.slug()
}

pub fn inspect(path: &Path, kind: ExpressionKind, builtin: bool) -> ResourceInfo {
    let mut info = ResourceInfo {
        path: path.to_owned(),
        builtin,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        version: String::new(),
        entries: 0,
        sources: Vec::new(),
        error: None,
    };
    match ExpressionLibrary::load(path, kind) {
        Ok(library) => {
            let document = library.document();
            info.name.clone_from(&document.name);
            info.version.clone_from(&document.version);
            info.entries = document.entries.len();
            info.sources.clone_from(&document.sources);
        }
        Err(error) => info.error = Some(error.to_string()),
    }
    info
}

/// List bundled, imported and explicitly selected files, including broken selections.
pub fn list(
    config: &Path,
    kind: ExpressionKind,
    selected: &ExpressionSelection,
) -> Result<Vec<ResourceInfo>, String> {
    let builtin = crate::config_path::resolve_resource_path(config, &kind.default_path());
    let mut paths = BTreeSet::from([builtin.clone(), selected.path(config, kind)]);
    let imports =
        crate::config_path::resolve_resource_path(config, Path::new("expressions/imports"));
    match std::fs::read_dir(&imports) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|error| error.to_string())?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{}-", slug(kind)))
                    && entry
                        .file_type()
                        .map_err(|error| error.to_string())?
                        .is_dir()
                {
                    let path = entry.path().join("library.mwe");
                    if path.is_file() {
                        paths.insert(path);
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法读取词库目录 {}: {error}", imports.display())),
    }
    let mut result: Vec<_> = paths
        .into_iter()
        .map(|path| inspect(&path, kind, path == builtin))
        .collect();
    result.sort_by(|a, b| {
        b.builtin
            .cmp(&a.builtin)
            .then(a.name.cmp(&b.name))
            .then(a.path.cmp(&b.path))
    });
    Ok(result)
}

/// Copy a validated document into a new private generation. Never overwrite a selected
/// resource or change configuration: the caller separately saves the user's selection.
pub fn import(config: &Path, kind: ExpressionKind, source: &Path) -> Result<ResourceInfo, String> {
    let library = ExpressionLibrary::load(source, kind).map_err(|error| error.to_string())?;
    let encoded = library.encode().map_err(|error| error.to_string())?;
    let root = crate::config_path::resolve_resource_path(config, Path::new("expressions/imports"));
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let directory = loop {
        let directory = root.join(format!(
            "{}-{stamp}-{}-{}",
            slug(kind),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&directory) {
            Ok(()) => break directory,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    let path = directory.join("library.mwe");
    if let Err(error) = crate::data::atomic_write(&path, encoded.as_bytes()) {
        let _ = std::fs::remove_dir(&directory);
        return Err(error.to_string());
    }
    let document = library.document();
    Ok(ResourceInfo {
        path,
        builtin: false,
        name: document.name.clone(),
        version: document.version.clone(),
        entries: document.entries.len(),
        sources: document.sources.clone(),
        error: None,
    })
}

/// Prepare a missing bundled file. Existing data, including user edits, is preserved.
pub fn prepare_builtin(config: &Path, bundle: &Path, kind: ExpressionKind) -> Result<(), String> {
    let destination = crate::config_path::resolve_resource_path(config, &kind.default_path());
    if destination.exists() {
        return Ok(());
    }
    let source = bundle.join(format!("{}.mwe", slug(kind)));
    let library = ExpressionLibrary::load(&source, kind).map_err(|error| error.to_string())?;
    let parent = destination.parent().ok_or("词库路径没有父目录")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    crate::data::atomic_write(
        &destination,
        library
            .encode()
            .map_err(|error| error.to_string())?
            .as_bytes(),
    )
    .map_err(|error| error.to_string())
}
