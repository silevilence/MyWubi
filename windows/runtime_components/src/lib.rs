//! Application-authorized native components, separate from data-only model packages.
//! No inference runtime or network client is linked by this crate.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("组件文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("组件清单、来源或依赖无效")]
    Manifest,
    #[error("组件与应用版本、系统或架构不兼容")]
    Incompatible,
    #[error("缺少推理组件，请在设置中下载")]
    Missing,
    #[error("组件文件大小或 SHA-256 校验失败")]
    Integrity,
    #[error("组件正在使用或仍被其他组件依赖")]
    InUse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub version: String,
    pub application: String,
    pub os: String,
    pub architecture: String,
    pub capabilities: Vec<String>,
    /// Exact component keys, installed before this component.
    pub dependencies: Vec<String>,
    pub source: String,
    pub license: String,
    pub archive: File,
    pub installed_bytes: u64,
    pub files: Vec<File>,
}

pub fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !s.starts_with('.')
        && !s.ends_with('.')
}

/// Reject traversal, alternate streams and Windows reserved names.
pub fn safe_path(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 240
        && s.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            identifier(part)
                && !["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.as_bytes()[3].is_ascii_digit())
        })
}
fn valid_file(f: &File) -> bool {
    safe_path(&f.path)
        && f.bytes > 0
        && f.bytes <= 2 * 1024 * 1024 * 1024
        && f.sha256.len() == 64
        && f.sha256.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Component {
    pub fn key(&self) -> String {
        format!("{}--{}", self.id, self.version)
    }
    pub fn validate(&self, application: &str, os: &str, arch: &str) -> Result<(), Error> {
        if !identifier(&self.id)
            || !identifier(&self.version)
            || self.license.is_empty()
            || !valid_file(&self.archive)
            || self.archive.path.contains('/')
            || self.files.is_empty()
            || self.files.len() > 256
            || !['v', 'V'].iter().any(|prefix| self.source == format!(
                "https://github.com/silevilence/MyWubi/releases/download/{prefix}{application}/{}", self.archive.path))
            || self.dependencies.iter().any(|d| !identifier(d))
        {
            return Err(Error::Manifest);
        }
        let required = semver::VersionReq::parse(&self.application).map_err(|_| Error::Manifest)?;
        let version = semver::Version::parse(application).map_err(|_| Error::Manifest)?;
        if !required.matches(&version) || self.os != os || self.architecture != arch {
            return Err(Error::Incompatible);
        }
        let mut names = BTreeSet::new();
        let mut bytes = 0u64;
        for file in &self.files {
            if !valid_file(file) || !names.insert(file.path.to_ascii_lowercase()) {
                return Err(Error::Manifest);
            }
            bytes = bytes.checked_add(file.bytes).ok_or(Error::Manifest)?;
        }
        if bytes != self.installed_bytes || bytes > 4 * 1024 * 1024 * 1024 {
            return Err(Error::Manifest);
        }
        if !self.capabilities.is_empty() && !names.contains("model-worker.exe") {
            return Err(Error::Manifest);
        }
        Ok(())
    }
}

/// Embedded by the application build, never supplied by a model or downloaded manifest.
pub fn catalog() -> Result<Vec<Component>, Error> {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/components.json")))
        .map_err(|_| Error::Manifest)
}
pub fn plan<'a>(
    catalog: &'a [Component],
    capability: &str,
    application: &str,
    os: &str,
    arch: &str,
) -> Result<Vec<&'a Component>, Error> {
    let component = catalog
        .iter()
        .find(|c| c.capabilities.iter().any(|s| s == capability))
        .ok_or(Error::Missing)?;
    plan_component(catalog, component, application, os, arch)
}

fn plan_component<'a>(
    catalog: &'a [Component],
    component: &'a Component,
    application: &str,
    os: &str,
    arch: &str,
) -> Result<Vec<&'a Component>, Error> {
    fn visit<'a>(
        c: &'a Component,
        all: &'a [Component],
        seen: &mut BTreeSet<String>,
        visiting: &mut BTreeSet<String>,
        result: &mut Vec<&'a Component>,
        target: (&str, &str, &str),
    ) -> Result<(), Error> {
        let key = c.key();
        if seen.contains(&key) {
            return Ok(());
        }
        if !visiting.insert(key.clone()) {
            return Err(Error::Manifest);
        }
        c.validate(target.0, target.1, target.2)?;
        for dependency in &c.dependencies {
            visit(
                all.iter()
                    .find(|d| d.key() == *dependency)
                    .ok_or(Error::Missing)?,
                all,
                seen,
                visiting,
                result,
                target,
            )?;
        }
        visiting.remove(&key);
        seen.insert(key);
        result.push(c);
        Ok(())
    }
    let mut keys = BTreeSet::new();
    if catalog
        .iter()
        .any(|c| !keys.insert(c.key().to_ascii_lowercase()))
    {
        return Err(Error::Manifest);
    }
    let mut result = Vec::new();
    visit(
        component,
        catalog,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut result,
        (application, os, arch),
    )?;
    Ok(result)
}
/// Persistent beside config.toml in portable and AppData layouts.
pub fn store(config: &Path) -> PathBuf {
    config.with_file_name("components")
}

/// Leases protect component removal while a service process owns native libraries.
pub fn lock(directory: &Path, name: &str, exclusive: bool) -> Result<fs::File, Error> {
    no_links(directory)?;
    let path = directory.join(name);
    if path.exists() {
        no_links(&path)?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    if exclusive {
        fs2::FileExt::try_lock_exclusive(&file)
    } else {
        fs2::FileExt::try_lock_shared(&file)
    }
    .map_err(|_| Error::InUse)?;
    Ok(file)
}

/// Keep leases outside the payload: Windows cannot rename a directory containing
/// an open lease file. Recheck after locking to reject concurrent removal.
pub fn component_lease(directory: &Path, exclusive: bool) -> Result<fs::File, Error> {
    no_links(directory)?;
    let parent = directory.parent().ok_or(Error::Manifest)?;
    let name = directory
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(Error::Manifest)?;
    if !identifier(name) {
        return Err(Error::Manifest);
    }
    let lease = lock(parent, &format!("{name}.lease"), exclusive)?;
    no_links(directory)?;
    Ok(lease)
}
pub fn no_links(path: &Path) -> Result<(), Error> {
    for p in path.ancestors() {
        let meta = fs::symlink_metadata(p)?;
        if meta.file_type().is_symlink() {
            return Err(Error::Manifest);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(Error::Manifest);
            }
        }
    }
    Ok(())
}
pub fn verify_file(path: &Path, file: &File) -> Result<(), Error> {
    no_links(path)?;
    if fs::metadata(path)?.len() != file.bytes {
        return Err(Error::Integrity);
    }
    let mut reader = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != file.sha256.to_ascii_lowercase() {
        return Err(Error::Integrity);
    }
    Ok(())
}
pub fn verify(root: &Path, component: &Component) -> Result<PathBuf, Error> {
    let directory = root.join(component.key());
    verify_directory(&directory, component)?;
    Ok(directory)
}

pub fn verify_directory(directory: &Path, component: &Component) -> Result<(), Error> {
    fn inventory(root: &Path, directory: &Path, paths: &mut BTreeSet<String>) -> Result<(), Error> {
        no_links(directory)?;
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            no_links(&path)?;
            if path.is_dir() {
                inventory(root, &path, paths)?;
            } else {
                paths.insert(
                    path.strip_prefix(root)
                        .map_err(|_| Error::Manifest)?
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        Ok(())
    }
    let mut actual = BTreeSet::new();
    inventory(directory, directory, &mut actual)?;
    actual.remove(".lease");
    actual.remove(".component.json");
    if actual != component.files.iter().map(|f| f.path.clone()).collect() {
        return Err(Error::Integrity);
    }
    for file in &component.files {
        if !valid_file(file) {
            return Err(Error::Manifest);
        }
        verify_file(&directory.join(&file.path), file)?;
    }
    Ok(())
}
/// Receipts support old-version inventory and removal, never executable authorization.
pub fn installed(root: &Path) -> Result<Vec<Component>, Error> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    no_links(root)?;
    let mut result = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(Error::Manifest)?;
        if !identifier(name) || !path.is_dir() {
            continue;
        }
        let receipt = path.join(".component.json");
        if !receipt.exists() {
            continue;
        }
        no_links(&receipt)?;
        if fs::metadata(&receipt)?.len() > 128 * 1024 {
            return Err(Error::Manifest);
        }
        let component: Component =
            serde_json::from_slice(&fs::read(receipt)?).map_err(|_| Error::Manifest)?;
        if component.key() != name || !identifier(&component.id) || !identifier(&component.version)
        {
            continue;
        }
        result.push(component);
    }
    Ok(result)
}

/// Background-thread resolution: verify application-authorized payloads before spawning.
pub fn worker(config: &Path, capability: &str) -> Result<PathBuf, Error> {
    let all = catalog()?;
    installed_worker(&store(config), &all, capability)
}

fn installed_worker(root: &Path, all: &[Component], capability: &str) -> Result<PathBuf, Error> {
    let mut failure = Error::Missing;
    // Catalog order expresses preference. A failed update must not hide a still
    // compatible, verified version explicitly retained by this application.
    for candidate in all
        .iter()
        .filter(|c| c.capabilities.iter().any(|c| c == capability))
    {
        let result = (|| {
            for component in plan_component(
                all,
                candidate,
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
            )? {
                verify(root, component)?;
            }
            Ok(root.join(candidate.key()).join("model-worker.exe"))
        })();
        match result {
            Ok(program) => return Ok(program),
            Err(error) => failure = error,
        }
    }
    Err(failure)
}

/// The only model-supplied input is a backend name; paths and sources remain application-owned.
pub fn model_capability(models: &Path, directory: &str) -> Result<&'static str, Error> {
    if !identifier(directory) {
        return Err(Error::Manifest);
    }
    let path = models.join(directory).join("manifest.json");
    no_links(&path)?;
    if fs::metadata(&path)?.len() > 65536 {
        return Err(Error::Manifest);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(path)?).map_err(|_| Error::Manifest)?;
    match value["backend"].as_str() {
        Some("LlamaCpp") => Ok("llama"),
        Some("OnnxRuntime") => Ok("onnx"),
        _ => Err(Error::Manifest),
    }
}

#[cfg(test)]
mod activation_tests {
    use super::*;
    #[test]
    fn failed_update_retains_only_an_explicitly_trusted_compatible_version() {
        let root = tempfile::tempdir().unwrap();
        let file = File {
            path: "model-worker.exe".into(),
            bytes: 1,
            sha256: format!("{:x}", Sha256::digest(b"x")),
        };
        let old = Component {
            id: "cpu".into(),
            version: "1".into(),
            application: format!("={}", env!("CARGO_PKG_VERSION")),
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            capabilities: vec!["llama".into()],
            dependencies: vec![],
            source: format!(
                "https://github.com/silevilence/MyWubi/releases/download/v{}/cpu.zip",
                env!("CARGO_PKG_VERSION")
            ),
            license: "test".into(),
            archive: File {
                path: "cpu.zip".into(),
                ..file.clone()
            },
            installed_bytes: 1,
            files: vec![file],
        };
        fs::create_dir(root.path().join(old.key())).unwrap();
        let program = root.path().join(old.key()).join("model-worker.exe");
        fs::write(&program, b"x").unwrap();
        let newer = Component {
            version: "2".into(),
            ..old.clone()
        };
        assert_eq!(
            installed_worker(root.path(), &[newer.clone(), old.clone()], "llama").unwrap(),
            program
        );
        assert!(installed_worker(root.path(), std::slice::from_ref(&newer), "llama").is_err());
        let incompatible = Component {
            application: "<0.1.0".into(),
            ..old
        };
        assert!(installed_worker(root.path(), &[newer, incompatible], "llama").is_err());
    }
}
