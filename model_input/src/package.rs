//! Data-only model packages. Validate every file before activation or atomic import.
use core_engine::assistance::{Capabilities, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const MAX_PACKAGE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backend {
    LlamaCpp,
    OnnxRuntime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub protocol: u32,
    pub id: String,
    pub version: String,
    pub backend: Backend,
    pub adapter: String,
    pub model: String,
    pub vocabulary: Option<String>,
    /// Optional hashed JSON admission policy, bound to these exact model weights.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_policy: Option<String>,
    pub files: Vec<Artifact>,
    pub source: String,
    pub license: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("package I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid package manifest")]
    Manifest,
    #[error("unsafe or unsupported package file")]
    Path,
    #[error("package file size or SHA-256 mismatch")]
    Integrity,
    #[error("package exceeds resource limits")]
    Limit,
    #[error("package already exists; select a different version")]
    Exists,
    #[error("cannot remove the selected model")]
    Active,
}

/// Returned paths are verified relative paths inside this directory, never executable code.
pub struct ValidatedPackage {
    pub root: PathBuf,
    pub manifest: Manifest,
}

fn generation_policy(
    root: &Path,
    manifest: &Manifest,
) -> Result<Option<crate::generation::GenerationPolicy>, PackageError> {
    let Some(name) = &manifest.generation_policy else {
        return Ok(None);
    };
    let artifact = manifest
        .files
        .iter()
        .find(|f| &f.path == name)
        .ok_or(PackageError::Manifest)?;
    if artifact.bytes > 64 * 1024 {
        return Err(PackageError::Limit);
    }
    let model = manifest
        .files
        .iter()
        .find(|f| f.path == manifest.model)
        .ok_or(PackageError::Manifest)?;
    let policy: crate::generation::GenerationPolicy =
        serde_json::from_slice(&fs::read(root.join(safe_path(name)?))?)
            .map_err(|_| PackageError::Manifest)?;
    if manifest.backend != Backend::LlamaCpp
        || manifest.adapter != crate::generation::ADAPTER
        || !manifest.capabilities.proposes_words
        || !policy.validates(&model.sha256)
    {
        return Err(PackageError::Manifest);
    }
    Ok(Some(policy))
}

fn prediction_policy(
    root: &Path,
    manifest: &Manifest,
) -> Result<Option<crate::prediction_policy::PredictionPolicy>, PackageError> {
    let Some(name) = &manifest.prediction_policy else {
        return Ok(None);
    };
    let path = safe_path(name)?;
    let artifact = manifest
        .files
        .iter()
        .find(|f| f.path == *name)
        .ok_or(PackageError::Manifest)?;
    if artifact.bytes > 64 * 1024 {
        return Err(PackageError::Limit);
    }
    let model = manifest
        .files
        .iter()
        .find(|f| f.path == manifest.model)
        .ok_or(PackageError::Manifest)?;
    let policy: crate::prediction_policy::PredictionPolicy =
        serde_json::from_slice(&fs::read(root.join(path))?).map_err(|_| PackageError::Manifest)?;
    let vocabulary = manifest
        .vocabulary
        .as_ref()
        .map(|name| {
            manifest
                .files
                .iter()
                .find(|f| &f.path == name)
                .ok_or(PackageError::Manifest)
        })
        .transpose()?;
    if !policy.validates(
        &manifest.adapter,
        &model.sha256,
        vocabulary.map(|f| f.sha256.as_str()),
    ) {
        return Err(PackageError::Manifest);
    }
    Ok(Some(policy))
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.contains("--")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn safe_path(value: &str) -> Result<&Path, PackageError> {
    if value.is_empty() || value.len() > 240 || value.contains(['\\', ':']) {
        return Err(PackageError::Path);
    }
    let path = Path::new(value);
    for component in value.split('/') {
        let stem = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let numbered = stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'));
        if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || numbered
            || component
                .chars()
                .any(|c| c.is_control() || "<>|?*\"".contains(c))
        {
            return Err(PackageError::Path);
        }
    }
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
        || value
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == ".." || c.ends_with(['.', ' ']))
    {
        return Err(PackageError::Path);
    }
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["gguf", "onnx", "json", "txt", "bin", "data", "license"].contains(&extension.as_str()) {
        return Err(PackageError::Path);
    }
    Ok(path)
}

/// Reject all symlinks/reparse points, including directories, before opening data files.
fn no_links(root: &Path, relative: &Path) -> Result<(), PackageError> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            return Err(PackageError::Path);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(PackageError::Path);
            }
        }
    }
    Ok(())
}

pub fn sha256(path: &Path) -> Result<String, PackageError> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn validate(root: &Path) -> Result<ValidatedPackage, PackageError> {
    let root = root.canonicalize()?;
    no_links(&root, Path::new("manifest.json"))?;
    if fs::metadata(root.join("manifest.json"))?.len() > 64 * 1024 {
        return Err(PackageError::Limit);
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(root.join("manifest.json"))?)
        .map_err(|_| PackageError::Manifest)?;
    if manifest.protocol != PROTOCOL_VERSION
        || manifest.capabilities.protocol != PROTOCOL_VERSION
        || !identifier(&manifest.id)
        || !identifier(&manifest.version)
        || manifest.source.trim().is_empty()
        || manifest.license.trim().is_empty()
        || manifest.files.is_empty()
        || manifest.files.len() > 32
    {
        return Err(PackageError::Manifest);
    }
    let supported = match manifest.backend {
        Backend::LlamaCpp => {
            (manifest.adapter == "gguf-continuation-v1"
                || manifest.adapter == crate::generation::ADAPTER)
                && manifest.vocabulary.is_none()
                && manifest.model.ends_with(".gguf")
        }
        Backend::OnnxRuntime => {
            manifest.adapter == "character-step-v1"
                && manifest.vocabulary.is_some()
                && manifest.model.ends_with(".onnx")
        }
    };
    if !supported {
        return Err(PackageError::Manifest);
    }
    if manifest.capabilities.proposes_words != manifest.generation_policy.is_some()
        || (manifest.adapter == crate::generation::ADAPTER) != manifest.generation_policy.is_some()
    {
        return Err(PackageError::Manifest);
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for file in &manifest.files {
        let relative = safe_path(&file.path)?;
        if file.path.eq_ignore_ascii_case("manifest.json")
            || !names.insert(file.path.to_ascii_lowercase())
        {
            return Err(PackageError::Manifest);
        }
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(PackageError::Manifest);
        }
        total = total.checked_add(file.bytes).ok_or(PackageError::Limit)?;
        if total > MAX_PACKAGE_BYTES {
            return Err(PackageError::Limit);
        }
        no_links(&root, relative)?;
        let path = root.join(relative);
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file()
            || metadata.len() != file.bytes
            || !sha256(&path)?.eq_ignore_ascii_case(&file.sha256)
        {
            return Err(PackageError::Integrity);
        }
    }
    if !names.contains(&manifest.model.to_ascii_lowercase())
        || manifest
            .vocabulary
            .as_ref()
            .is_some_and(|v| !names.contains(&v.to_ascii_lowercase()))
    {
        return Err(PackageError::Manifest);
    }
    safe_path(&manifest.model)?;
    if let Some(vocab) = &manifest.vocabulary {
        safe_path(vocab)?;
    }
    reject_unlisted(&root, &root, &names)?;
    prediction_policy(&root, &manifest)?;
    generation_policy(&root, &manifest)?;
    Ok(ValidatedPackage { root, manifest })
}

fn reject_unlisted(
    root: &Path,
    directory: &Path,
    names: &BTreeSet<String>,
) -> Result<(), PackageError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|_| PackageError::Path)?;
        no_links(root, relative)?;
        let name = relative
            .to_str()
            .ok_or(PackageError::Path)?
            .replace('\\', "/")
            .to_ascii_lowercase();
        if entry.file_type()?.is_dir() {
            let prefix = format!("{name}/");
            if !names.iter().any(|file| file.starts_with(&prefix)) {
                return Err(PackageError::Path);
            }
            reject_unlisted(root, &path, names)?;
        } else {
            if name != "manifest.json" && !names.contains(&name) {
                return Err(PackageError::Path);
            }
        }
    }
    Ok(())
}

/// Import into a caller-supplied application data directory. Nothing is activated here.
/// Failure never replaces an installed version; each import has a fresh staging directory.
pub fn import(source: &Path, store: &Path) -> Result<PathBuf, PackageError> {
    let package = validate(source)?;
    fs::create_dir_all(store)?;
    let store = store.canonicalize()?;
    let name = format!("{}--{}", package.manifest.id, package.manifest.version);
    let destination = store.join(&name);
    if destination.exists() {
        return Err(PackageError::Exists);
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| PackageError::Manifest)?
        .as_nanos();
    let staging = store.join(format!(".import-{}-{nonce}", std::process::id()));
    fs::create_dir(&staging)?;
    let result = (|| {
        for file in &package.manifest.files {
            let path = staging.join(safe_path(&file.path)?);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(package.root.join(&file.path), &path)?;
        }
        let manifest =
            serde_json::to_vec_pretty(&package.manifest).map_err(|_| PackageError::Manifest)?;
        let mut file = fs::File::create(staging.join("manifest.json"))?;
        file.write_all(&manifest)?;
        file.sync_all()?;
        // Windows cannot rename a directory while this manifest handle is open.
        drop(file);
        validate(&staging)?;
        fs::rename(&staging, &destination)?;
        Ok(destination)
    })();
    if result.is_err()
        && staging.is_dir()
        && staging.canonicalize()?.parent() == Some(store.as_path())
    {
        fs::remove_dir_all(&staging)?;
    }
    result
}

pub fn list(store: &Path) -> Result<Vec<PathBuf>, PackageError> {
    if !store.exists() {
        return Ok(vec![]);
    }
    let store = store.canonicalize()?;
    let mut packages = Vec::new();
    for entry in fs::read_dir(&store)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && !entry.file_name().to_string_lossy().starts_with('.')
            && validate(&entry.path()).is_ok()
        {
            packages.push(entry.path());
        }
    }
    packages.sort();
    Ok(packages)
}

/// Caller supplies the selected directory; active versions cannot be removed.
pub fn remove(
    store: &Path,
    id: &str,
    version: &str,
    selected: Option<&Path>,
) -> Result<(), PackageError> {
    if !identifier(id) || !identifier(version) {
        return Err(PackageError::Path);
    }
    let root = store.canonicalize()?;
    let name = format!("{id}--{version}");
    no_links(&root, Path::new(&name))?;
    let target = root.join(name).canonicalize()?;
    if target.parent() != Some(root.as_path()) {
        return Err(PackageError::Path);
    }
    if let Some(selected) = selected {
        if selected.canonicalize()? == target {
            return Err(PackageError::Active);
        }
    }
    fs::remove_dir_all(target)?;
    Ok(())
}

/// Construct only built-in adapters. File contents never choose a native plugin/library.
pub fn load(
    package: &ValidatedPackage,
) -> Result<Box<dyn core_engine::assistance::ModelBackend>, core_engine::assistance::Unavailable> {
    load_with_threads(package, 2)
}

pub fn load_with_threads(
    package: &ValidatedPackage,
    threads: u8,
) -> Result<Box<dyn core_engine::assistance::ModelBackend>, core_engine::assistance::Unavailable> {
    use core_engine::assistance::Unavailable;
    if !(1..=4).contains(&threads) {
        return Err(Unavailable::Limit);
    }
    // Revalidate at the activation boundary; an old validation handle is not a trust token.
    let checked = validate(&package.root).map_err(|_| Unavailable::Load)?;
    let model = checked.root.join(&checked.manifest.model);
    let backend: Result<Box<dyn core_engine::assistance::ModelBackend>, Unavailable> =
        match checked.manifest.backend {
            Backend::LlamaCpp => {
                #[cfg(feature = "llama-cpp-2")]
                {
                    let scorer = crate::llama::LlamaScorer::load(&model, 1024, i32::from(threads))?;
                    let scorer = match generation_policy(&checked.root, &checked.manifest)
                        .map_err(|_| Unavailable::Load)?
                    {
                        Some(policy) => scorer.with_generation(policy),
                        None => scorer,
                    };
                    Ok(Box::new(scorer))
                }
                #[cfg(not(feature = "llama-cpp-2"))]
                {
                    let _ = model;
                    Err(Unavailable::Capability)
                }
            }
            Backend::OnnxRuntime => {
                #[cfg(feature = "ort")]
                {
                    let vocabulary = checked
                        .manifest
                        .vocabulary
                        .as_ref()
                        .ok_or(Unavailable::UnsupportedModel)?;
                    Ok(Box::new(crate::onnx::OnnxScorer::load(
                        &model,
                        &checked.root.join(vocabulary),
                        usize::from(threads),
                    )?))
                }
                #[cfg(not(feature = "ort"))]
                {
                    let _ = model;
                    Err(Unavailable::Capability)
                }
            }
        };
    let backend = backend?;
    if backend.capabilities() != &checked.manifest.capabilities {
        return Err(Unavailable::Capability);
    }
    Ok(Box::new(crate::prediction_policy::GatedBackend {
        policy: prediction_policy(&checked.root, &checked.manifest)
            .map_err(|_| Unavailable::Load)?,
        backend,
    }))
}

/// Owns one selected model on a service/worker thread. Loading a replacement is
/// transactional: a failed validation/load leaves the previous model usable.
/// Selection persistence belongs to the platform configuration, using id/version.
pub struct ModelStore {
    root: PathBuf,
    generation: u64,
    active: Option<ActiveModel>,
}
struct ActiveModel {
    package: ValidatedPackage,
    session: core_engine::assistance::ModelSession<Box<dyn core_engine::assistance::ModelBackend>>,
}
#[derive(Debug, thiserror::Error)]
pub enum SelectionError {
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error(transparent)]
    Model(#[from] core_engine::assistance::Unavailable),
}
impl ModelStore {
    pub fn open(root: &Path) -> Result<Self, PackageError> {
        fs::create_dir_all(root)?;
        Ok(Self {
            root: root.canonicalize()?,
            generation: 0,
            active: None,
        })
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn selected(&self) -> Option<&Manifest> {
        self.active.as_ref().map(|a| &a.package.manifest)
    }
    pub fn import(&self, source: &Path) -> Result<PathBuf, PackageError> {
        import(source, &self.root)
    }
    pub fn list(&self) -> Result<Vec<PathBuf>, PackageError> {
        list(&self.root)
    }
    pub fn remove(&self, id: &str, version: &str) -> Result<(), PackageError> {
        remove(
            &self.root,
            id,
            version,
            self.active.as_ref().map(|a| a.package.root.as_path()),
        )
    }
    /// Must run off the input thread. Peak memory can temporarily include both
    /// models; the service must apply its device budget before attempting a swap.
    pub fn select(&mut self, id: &str, version: &str) -> Result<u64, SelectionError> {
        if !identifier(id) || !identifier(version) {
            return Err(PackageError::Path.into());
        }
        let name = format!("{id}--{version}");
        no_links(&self.root, Path::new(&name))?;
        let package = validate(&self.root.join(name))?;
        if package.manifest.id != id || package.manifest.version != version {
            return Err(PackageError::Manifest.into());
        }
        let backend = load(&package)?;
        let session = core_engine::assistance::ModelSession::new(backend);
        self.active = Some(ActiveModel { package, session });
        self.generation = self.generation.wrapping_add(1);
        Ok(self.generation)
    }
    pub fn unload(&mut self) {
        self.active = None;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn score(
        &mut self,
        request: &core_engine::assistance::ScoreRequest,
        control: &core_engine::assistance::ComputeControl,
    ) -> Result<core_engine::assistance::ScoreResponse, core_engine::assistance::Unavailable> {
        use core_engine::assistance::Unavailable;
        if request.key.model != self.generation {
            return Err(Unavailable::Stale);
        }
        self.active
            .as_mut()
            .ok_or(Unavailable::MissingModel)?
            .session
            .score(request, control)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_engine::assistance::Task;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "mywubi-model-package-{}-{}-{nonce}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn package(&self) -> PathBuf {
            let root = self.0.join("source");
            fs::create_dir(&root).unwrap();
            fs::write(
                root.join("model.gguf"),
                b"test fixture, not a runtime model",
            )
            .unwrap();
            let manifest = Manifest {
                protocol: 1,
                id: "fixture".into(),
                version: "v1".into(),
                backend: Backend::LlamaCpp,
                adapter: "gguf-continuation-v1".into(),
                model: "model.gguf".into(),
                vocabulary: None,
                prediction_policy: None,
                generation_policy: None,
                files: vec![Artifact {
                    path: "model.gguf".into(),
                    bytes: fs::metadata(root.join("model.gguf")).unwrap().len(),
                    sha256: sha256(&root.join("model.gguf")).unwrap(),
                }],
                source: "authored compatibility fixture".into(),
                license: "MIT".into(),
                capabilities: Capabilities {
                    protocol: 1,
                    tasks: vec![Task::Rank],
                    languages: vec!["zh".into()],
                    max_context_chars: 256,
                    max_candidates: 64,
                    max_candidate_chars: 16,
                    max_code_chars: 64,
                    proposes_words: false,
                    incremental: true,
                    cooperative_cancel: true,
                },
            };
            fs::write(
                root.join("manifest.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            root
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.0.parent() == Some(std::env::temp_dir().as_path()) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn corruption_failed_import_and_active_removal_preserve_installed_package() {
        let f = Fixture::new();
        let source = f.package();
        let store = f.0.join("store");
        let installed = import(&source, &store).unwrap();
        assert_eq!(
            list(&store).unwrap().as_slice(),
            std::slice::from_ref(&installed)
        );
        assert!(matches!(import(&source, &store), Err(PackageError::Exists)));
        assert!(matches!(
            remove(&store, "fixture", "v1", Some(&installed)),
            Err(PackageError::Active)
        ));
        fs::write(source.join("model.gguf"), b"damaged").unwrap();
        assert!(matches!(
            import(&source, &store),
            Err(PackageError::Integrity)
        ));
        validate(&installed).unwrap();
        remove(&store, "fixture", "v1", None).unwrap();
        assert!(list(&store).unwrap().is_empty());
    }
    #[test]
    fn generation_policy_is_independent_and_bound_to_exact_gguf() {
        let f = Fixture::new();
        let source = f.package();
        let mut manifest = validate(&source).unwrap().manifest;
        manifest.adapter = crate::generation::ADAPTER.into();
        manifest.capabilities.proposes_words = true;
        manifest.generation_policy = Some("generation.json".into());
        let mut policy: crate::generation::GenerationPolicy =
            serde_json::from_str(include_str!("../evaluation/generation-policy.json")).unwrap();
        policy.model_sha256 = manifest.files[0].sha256.clone();
        let path = source.join("generation.json");
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        manifest.files.push(Artifact {
            path: "generation.json".into(),
            bytes: fs::metadata(&path).unwrap().len(),
            sha256: sha256(&path).unwrap(),
        });
        fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(validate(&source).is_ok());
        policy.model_sha256 = "b".repeat(64);
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        manifest.files[1].bytes = fs::metadata(&path).unwrap().len();
        manifest.files[1].sha256 = sha256(&path).unwrap();
        fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(matches!(validate(&source), Err(PackageError::Manifest)));
    }

    #[test]
    fn prediction_policy_requires_hash_integrity_and_the_exact_model() {
        let f = Fixture::new();
        let source = f.package();
        let mut manifest = validate(&source).unwrap().manifest;
        let mut policy = crate::prediction_policy::PredictionPolicy {
            version: 1,
            adapter: manifest.adapter.clone(),
            model_sha256: manifest.files[0].sha256.clone(),
            vocabulary_sha256: None,
            minimum_score: -2.,
            minimum_margin: 0.5,
            evidence: "Authored regression fixture, no product quality claim".into(),
        };
        let path = source.join("prediction-policy.json");
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        manifest.prediction_policy = Some("prediction-policy.json".into());
        manifest.files.push(Artifact {
            path: "prediction-policy.json".into(),
            bytes: fs::metadata(&path).unwrap().len(),
            sha256: sha256(&path).unwrap(),
        });
        fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(validate(&source).is_ok());
        policy.model_sha256 = "b".repeat(64);
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        assert!(matches!(validate(&source), Err(PackageError::Integrity)));
        manifest.files[1].bytes = fs::metadata(&path).unwrap().len();
        manifest.files[1].sha256 = sha256(&path).unwrap();
        fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(matches!(validate(&source), Err(PackageError::Manifest)));
    }

    #[test]
    fn rejects_traversal_native_code_duplicate_case_names_and_unlisted_files() {
        for path in [
            "../outside.gguf",
            "C:/weights.gguf",
            "dir\\file.gguf",
            "/absolute.gguf",
            "dir/../file.gguf",
            "run.dll",
            "run.py",
            "x//a.gguf",
        ] {
            assert!(safe_path(path).is_err(), "{path}");
        }
        let f = Fixture::new();
        let source = f.package();
        fs::write(source.join("plugin.dll"), b"unused").unwrap();
        assert!(matches!(validate(&source), Err(PackageError::Path)));
        let f = Fixture::new();
        let source = f.package();
        let mut manifest = validate(&source).unwrap().manifest;
        let mut duplicate = manifest.files[0].clone();
        duplicate.path = duplicate.path.to_ascii_uppercase();
        manifest.files.push(duplicate);
        fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(matches!(validate(&source), Err(PackageError::Manifest)));
        let f = Fixture::new();
        let source = f.package();
        fs::create_dir(source.join("unlisted-empty-directory")).unwrap();
        assert!(matches!(validate(&source), Err(PackageError::Path)));
    }

    #[cfg(feature = "ort")]
    #[test]
    fn failed_switch_preserves_live_model_and_success_invalidates_old_requests() {
        use core_engine::assistance::{Cancellation, ComputeControl, ScoreRequest, Unavailable};
        use std::time::{Duration, Instant};
        let f = Fixture::new();
        let old_source = f.package();
        let mut manifest = validate(&old_source).unwrap().manifest;
        manifest.backend = Backend::OnnxRuntime;
        manifest.adapter = "character-step-v1".into();
        manifest.model = "model.onnx".into();
        manifest.vocabulary = Some("vocabulary.json".into());
        manifest.capabilities.tasks.push(Task::Predict);
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("evaluation/onnx");
        let mut store = ModelStore::open(&f.0.join("store")).unwrap();
        for kind in ["counts", "lstm", "bad-operator"] {
            let source = f.0.join(kind);
            fs::create_dir(&source).unwrap();
            manifest.id = kind.into();
            fs::copy(
                fixtures.join(format!("{kind}.onnx")),
                source.join("model.onnx"),
            )
            .unwrap();
            fs::copy(
                fixtures.join("vocabulary.json"),
                source.join("vocabulary.json"),
            )
            .unwrap();
            manifest.files = ["model.onnx", "vocabulary.json"]
                .into_iter()
                .map(|name| Artifact {
                    path: name.into(),
                    bytes: fs::metadata(source.join(name)).unwrap().len(),
                    sha256: sha256(&source.join(name)).unwrap(),
                })
                .collect();
            fs::write(
                source.join("manifest.json"),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            store.import(&source).unwrap();
        }
        let mut request: ScoreRequest =
            serde_json::from_str(include_str!("../evaluation/request.json")).unwrap();
        let control = ComputeControl {
            cancellation: Cancellation::default(),
            deadline: Instant::now() + Duration::from_secs(10),
        };
        request.key.model = store.select("counts", "v1").unwrap();
        let before = store.score(&request, &control).unwrap();
        assert!(matches!(
            store.remove("counts", "v1"),
            Err(PackageError::Active)
        ));
        assert!(store.select("bad-operator", "v1").is_err());
        assert_eq!(store.selected().unwrap().id, "counts");
        assert_eq!(before, store.score(&request, &control).unwrap());
        let generation = store.select("lstm", "v1").unwrap();
        assert_eq!(store.score(&request, &control), Err(Unavailable::Stale));
        request.key.model = generation;
        assert!(store.score(&request, &control).is_ok());
        store.remove("counts", "v1").unwrap();
        store.unload();
        request.key.model = store.generation();
        assert_eq!(
            store.score(&request, &control),
            Err(Unavailable::MissingModel)
        );
        store.remove("lstm", "v1").unwrap();
    }
}
