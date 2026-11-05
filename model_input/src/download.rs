//! Resumable, data-only downloads. Installation never changes input configuration.
use crate::package::{self, Artifact, Manifest};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    HuggingFace,
    HfMirror,
    ModelScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMapping {
    pub repository: String,
    pub revision: String,
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFile {
    pub local: String,
    pub repository: String,
    pub revision: String,
    pub file: String,
    #[serde(default)]
    pub modelscope: Option<SourceMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadModel {
    pub manifest: Manifest,
    pub title: String,
    pub memory_bytes: u64,
    pub test_status: String,
    pub remote: Vec<RemoteFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub phase: String,
    pub completed: u64,
    pub total: u64,
    pub file: String,
    pub source: Source,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("下载已取消；已下载部分保留，重试时续传")]
    Cancelled,
    #[error("空间不足：需要至少 {required} 字节，可用 {available} 字节；请清理空间后重试")]
    Space { required: u64, available: u64 },
    #[error("源或固定版本不可用：{0}；请重试或选择已映射的其他源")]
    Network(String),
    #[error("下载记录或制品身份不匹配；没有更改原模型")]
    Identity,
    #[error("ModelScope 同版本映射不可用：{0}；请选择其他源，不能替换为相近模型")]
    Mapping(String),
    #[error("文件完整性校验失败：{0}；损坏部分已清空，可重新下载")]
    Integrity(String),
    #[error("已有同模型下载任务，请先取消或等待完成")]
    Busy,
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("模型包校验失败：{0}")]
    Package(#[from] package::PackageError),
}

pub fn available_models() -> Vec<DownloadModel> {
    // This is a compiled, reviewed catalog, never code or a remotely supplied manifest.
    serde_json::from_str(include_str!("../catalog/models.json")).unwrap_or_default()
}

impl RemoteFile {
    fn location(&self, source: Source) -> Result<(&str, &str, &str), DownloadError> {
        if source == Source::ModelScope {
            let mapped = self
                .modelscope
                .as_ref()
                .ok_or_else(|| DownloadError::Mapping(format!("{} 没有映射", self.local)))?;
            Ok((&mapped.repository, &mapped.revision, &mapped.file))
        } else {
            Ok((&self.repository, &self.revision, &self.file))
        }
    }
    pub fn url(&self, source: Source) -> Result<String, DownloadError> {
        let (repository, revision, file) = self.location(source)?;
        let safe = |s: &str| {
            !s.is_empty()
                && s.split('/').all(|p| {
                    !p.is_empty()
                        && p != "."
                        && p != ".."
                        && p.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                })
        };
        if !safe(repository)
            || !safe(file)
            || revision.len() != 40
            || !revision.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(DownloadError::Identity);
        }
        let host = match source {
            Source::HuggingFace => "huggingface.co",
            Source::HfMirror => "hf-mirror.com",
            Source::ModelScope => return Ok(format!("https://modelscope.cn/api/v1/models/{repository}/repo?Revision={revision}&FilePath={file}")),
        };
        Ok(format!(
            "https://{host}/{}/resolve/{}/{}?download=true",
            repository, revision, file
        ))
    }
}

impl DownloadModel {
    pub fn validate_source(&self, source: Source) -> Result<(), DownloadError> {
        for remote in &self.remote {
            remote.url(source)?;
            let artifact = self
                .manifest
                .files
                .iter()
                .find(|f| f.path == remote.local)
                .ok_or(DownloadError::Identity)?;
            if source == Source::ModelScope {
                let mapping = remote
                    .modelscope
                    .as_ref()
                    .ok_or_else(|| DownloadError::Mapping(remote.local.clone()))?;
                if mapping.sha256 != artifact.sha256 || mapping.bytes != artifact.bytes {
                    return Err(DownloadError::Mapping(format!(
                        "{} 的哈希或长度不匹配",
                        remote.local
                    )));
                }
            }
        }
        Ok(())
    }
}

fn cancelled(cancel: &AtomicBool) -> Result<(), DownloadError> {
    if cancel.load(Ordering::Relaxed) {
        Err(DownloadError::Cancelled)
    } else {
        Ok(())
    }
}

fn check_space(required: u64, available: u64) -> Result<(), DownloadError> {
    if available < required {
        Err(DownloadError::Space {
            required,
            available,
        })
    } else {
        Ok(())
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(20)))
        .max_redirects(8)
        .build()
        .into()
}

/// Download a single pinned artifact. Only verified bytes can be installed.
fn transfer(
    agent: &ureq::Agent,
    url: &str,
    path: &Path,
    artifact: &Artifact,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64),
) -> Result<(), DownloadError> {
    cancelled(cancel)?;
    let mut offset = fs::metadata(path).map_or(0, |m| m.len());
    if offset == artifact.bytes && package::sha256(path)? == artifact.sha256 {
        progress(offset);
        return Ok(());
    }
    if offset >= artifact.bytes {
        File::create(path)?;
        offset = 0;
    }
    while offset < artifact.bytes {
        cancelled(cancel)?;
        let end = (offset + 8 * 1024 * 1024 - 1).min(artifact.bytes - 1);
        let mut response = agent
            .get(url)
            .header("Accept-Encoding", "identity")
            .header("Range", &format!("bytes={offset}-{end}"))
            .call()
            .map_err(|e| DownloadError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let expected_end = if status == 206 {
            let range = response
                .headers()
                .get("content-range")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("");
            if range != format!("bytes {offset}-{end}/{}", artifact.bytes) {
                return Err(DownloadError::Identity);
            }
            end + 1
        } else if status == 200 {
            offset = 0;
            artifact.bytes
        } else {
            return Err(DownloadError::Network(format!("HTTP {status}")));
        };
        let mut output = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(offset == 0)
            .append(offset != 0)
            .open(path)?;
        let mut reader = response.body_mut().as_reader();
        let mut buffer = [0u8; 65536];
        progress(offset);
        loop {
            cancelled(cancel)?;
            let n = reader
                .read(&mut buffer)
                .map_err(|e| DownloadError::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            if offset + n as u64 > expected_end {
                return Err(DownloadError::Identity);
            }
            output.write_all(&buffer[..n])?;
            offset += n as u64;
            progress(offset);
        }
        output.sync_all()?;
        if offset != expected_end {
            return Err(DownloadError::Network("连接中断，保留部分文件".into()));
        }
    }
    cancelled(cancel)?;
    if package::sha256(path)? != artifact.sha256 {
        File::create(path)?;
        return Err(DownloadError::Integrity(artifact.path.clone()));
    }
    Ok(())
}

/// The caller runs this on a background thread. Partial files live outside the selectable catalog.
pub fn download(
    model: &DownloadModel,
    source: Source,
    store: &Path,
    cancel: &AtomicBool,
    mut publish: impl FnMut(Progress),
) -> Result<PathBuf, DownloadError> {
    model.validate_source(source)?;
    let selection = core_engine::config::ModelSelection {
        id: model.manifest.id.clone(),
        version: model.manifest.version.clone(),
    };
    let name = selection.directory_name().ok_or(DownloadError::Identity)?;
    fs::create_dir_all(store)?;
    let store = store.canonicalize()?;
    let downloads = store.join(".downloads");
    fs::create_dir_all(&downloads)?;
    package::no_links(&store, Path::new(".downloads"))?;
    let lock_path = downloads.join(format!("{name}.lock"));
    if lock_path.exists() {
        package::no_links(&downloads, Path::new(&format!("{name}.lock")))?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| DownloadError::Busy)?;
    let workspace = downloads.join(&name);
    fs::create_dir_all(&workspace)?;
    package::no_links(&downloads, Path::new(&name))?;
    let stage = workspace.join("package");
    fs::create_dir_all(&stage)?;
    package::no_links(&workspace, Path::new("package"))?;
    let identity = serde_json::to_vec(&model.manifest).map_err(|_| DownloadError::Identity)?;
    let identity_path = workspace.join("identity.json");
    if identity_path.exists() {
        package::no_links(&workspace, Path::new("identity.json"))?;
        if fs::read(&identity_path)? != identity {
            return Err(DownloadError::Identity);
        }
    } else {
        fs::write(&identity_path, identity)?;
    }
    let mut total = 0u64;
    let mut remaining = 0u64;
    for file in &model.manifest.files {
        let relative = package::safe_path(&file.path)?;
        // Catalog downloads are flat; imported packages can still have safe subdirectories.
        if relative.components().count() != 1 {
            return Err(DownloadError::Identity);
        }
        let path = stage.join(relative);
        if path.exists() {
            package::no_links(&stage, relative)?;
        }
        total = total
            .checked_add(file.bytes)
            .ok_or(DownloadError::Identity)?;
        remaining += file
            .bytes
            .saturating_sub(fs::metadata(path).map_or(0, |m| m.len()));
    }
    // Import uses a fresh, atomically renamed copy; reserve that copy as well.
    check_space(
        total + remaining + 64 * 1024 * 1024,
        fs2::available_space(&store)?,
    )?;
    let agent = agent();
    let mut completed = 0;
    let mut sources = Vec::new();
    for file in &model.manifest.files {
        cancelled(cancel)?;
        let path = stage.join(&file.path);
        if file.path == "generation-policy.json" {
            fs::write(
                &path,
                include_bytes!("../evaluation/generation-policy.json"),
            )?;
        } else {
            let remote = model
                .remote
                .iter()
                .find(|r| r.local == file.path)
                .ok_or(DownloadError::Identity)?;
            let url = remote.url(source)?;
            // Persist source attempts before I/O so cancellation and cross-source resume are traceable.
            let (repository, revision, _) = remote.location(source)?;
            let attempt = serde_json::json!({"source":source,"url":url,"repository":repository,"revision":revision,"sha256":file.sha256,"offset":fs::metadata(&path).map_or(0,|m|m.len())});
            let attempts_path = workspace.join("attempts.jsonl");
            if attempts_path.exists() {
                package::no_links(&workspace, Path::new("attempts.jsonl"))?;
            }
            let mut attempts = OpenOptions::new()
                .create(true)
                .append(true)
                .open(attempts_path)?;
            writeln!(attempts, "{attempt}")?;
            sources.push(attempt);
            transfer(&agent, &url, &path, file, cancel, &mut |n| {
                publish(Progress {
                    phase: "downloading".into(),
                    completed: completed + n,
                    total,
                    file: file.path.clone(),
                    source,
                })
            })?;
        }
        completed += file.bytes;
    }
    publish(Progress {
        phase: "verifying".into(),
        completed,
        total,
        file: String::new(),
        source,
    });
    let manifest_path = stage.join("manifest.json");
    if manifest_path.exists() {
        package::no_links(&stage, Path::new("manifest.json"))?;
    }
    fs::write(
        manifest_path,
        serde_json::to_vec_pretty(&model.manifest).map_err(|_| DownloadError::Identity)?,
    )?;
    package::validate(&stage)?;
    cancelled(cancel)?;
    let receipts = store.join(".receipts");
    fs::create_dir_all(&receipts)?;
    package::no_links(&store, Path::new(".receipts"))?;
    let receipt_name = format!("{name}.json");
    if receipts.join(&receipt_name).exists() {
        package::no_links(&receipts, Path::new(&receipt_name))?;
    }
    let receipt = serde_json::json!({"identity":selection,"files":model.manifest.files,"source":source,"sources":sources,"attempts":fs::read_to_string(workspace.join("attempts.jsonl"))?});
    fs::write(
        receipts.join(receipt_name),
        serde_json::to_vec_pretty(&receipt).map_err(|_| DownloadError::Identity)?,
    )?;
    let installed = package::import(&stage, &store)?;
    publish(Progress {
        phase: "ready".into(),
        completed,
        total,
        file: String::new(),
        source,
    });
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::net::TcpListener;

    fn data() -> Vec<u8> {
        (0..200_000).map(|n| (n % 251) as u8).collect()
    }
    fn artifact(bytes: &[u8]) -> Artifact {
        Artifact {
            path: "model.gguf".into(),
            bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
    fn server(bytes: Vec<u8>, fault: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let fault = fault.to_owned();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                stream.read_exact(&mut b).unwrap();
                header.push(b[0]);
            }
            let header = String::from_utf8(header).unwrap().to_ascii_lowercase();
            let range = header
                .lines()
                .find_map(|l| l.strip_prefix("range: bytes="))
                .unwrap();
            let start: usize = range.split('-').next().unwrap().parse().unwrap();
            let start = if fault == "ignore" { 0 } else { start };
            let mut body = bytes[start..].to_vec();
            if fault == "corrupt" {
                body[0] ^= 1;
            }
            let size = body.len();
            if fault == "disconnect" {
                body.truncate(size / 2);
            }
            let response = if fault == "missing" {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
            } else if fault == "ignore" {
                format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n")
            } else {
                let declared = if fault == "range" { start + 1 } else { start };
                format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {size}\r\nContent-Range: bytes {declared}-{}/{}\r\nConnection: close\r\n\r\n",bytes.len()-1,bytes.len())
            };
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
            header
        });
        (url, thread)
    }
    fn fetch(
        path: &Path,
        fault: &str,
        cancel_at: Option<u64>,
    ) -> (Result<(), DownloadError>, String) {
        let bytes = data();
        let expected = artifact(&bytes);
        let (url, server) = server(bytes, fault);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(4)))
            .build()
            .into();
        let cancel = AtomicBool::new(false);
        let result = transfer(&agent, &url, path, &expected, &cancel, &mut |n| {
            if cancel_at.is_some_and(|limit| n >= limit) {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        (result, server.join().unwrap())
    }
    #[test]
    fn cancel_and_new_source_resume_the_same_pinned_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.gguf");
        assert!(matches!(
            fetch(&path, "", Some(65536)).0,
            Err(DownloadError::Cancelled)
        ));
        let offset = fs::metadata(&path).unwrap().len();
        assert!(offset > 0 && offset < 200000);
        let (result, header) = fetch(&path, "", None);
        result.unwrap();
        assert!(header.contains(&format!("range: bytes={offset}-")));
        assert_eq!(fs::read(path).unwrap(), data());
    }
    #[test]
    fn disconnected_transfer_keeps_partial_and_retry_completes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.gguf");
        assert!(matches!(
            fetch(&path, "disconnect", None).0,
            Err(DownloadError::Network(_))
        ));
        assert!(fs::metadata(&path).unwrap().len() > 0);
        fetch(&path, "", None).0.unwrap();
        assert_eq!(fs::read(path).unwrap(), data());
    }
    #[test]
    fn corrupted_file_is_not_accepted_and_retry_starts_clean() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.gguf");
        assert!(matches!(
            fetch(&path, "corrupt", None).0,
            Err(DownloadError::Integrity(_))
        ));
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        fetch(&path, "", None).0.unwrap();
    }
    #[test]
    fn incorrect_range_is_rejected_without_appending_and_ignored_range_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.gguf");
        fs::write(&path, &data()[..1234]).unwrap();
        assert!(matches!(
            fetch(&path, "range", None).0,
            Err(DownloadError::Identity)
        ));
        assert_eq!(fs::metadata(&path).unwrap().len(), 1234);
        fetch(&path, "ignore", None).0.unwrap();
        assert_eq!(fs::read(path).unwrap(), data());
    }
    #[test]
    fn space_gate_and_pinned_source_mapping_are_explicit() {
        assert!(matches!(
            check_space(100, 99),
            Err(DownloadError::Space { .. })
        ));
        check_space(100, 100).unwrap();
        let models = available_models();
        assert_eq!(models.len(), 1);
        let model = &models[0];
        for remote in &model.remote {
            assert_eq!(
                remote
                    .url(Source::HuggingFace)
                    .unwrap()
                    .replace("huggingface.co", "hf-mirror.com"),
                remote.url(Source::HfMirror).unwrap()
            );
            let mut moving = remote.clone();
            moving.revision = "main".into();
            assert!(matches!(
                moving.url(Source::HuggingFace),
                Err(DownloadError::Identity)
            ));
        }
        let policy = model
            .manifest
            .files
            .iter()
            .find(|f| f.path == "generation-policy.json")
            .unwrap();
        assert_eq!(
            policy.sha256,
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../evaluation/generation-policy.json"))
            )
        );
    }

    #[test]
    fn modelscope_requires_an_explicit_identical_artifact_and_fixed_revision() {
        let mut model = available_models().remove(0);
        model.validate_source(Source::ModelScope).unwrap();
        let url = model.remote[0].url(Source::ModelScope).unwrap();
        assert!(url.contains("/api/v1/models/Qwen/Qwen2.5-0.5B-Instruct-GGUF/repo?Revision=4537f71740dd2c56a15ffa12e5fbb9f15095f20c&FilePath="));
        model.remote[0].modelscope.as_mut().unwrap().sha256 = "0".repeat(64);
        assert!(matches!(
            model.validate_source(Source::ModelScope),
            Err(DownloadError::Mapping(_))
        ));
        model.validate_source(Source::HuggingFace).unwrap();
        model.remote[0].modelscope = None;
        assert!(matches!(
            model.validate_source(Source::ModelScope),
            Err(DownloadError::Mapping(_))
        ));
        model.remote[0].modelscope = available_models().remove(0).remote.remove(0).modelscope;
        model.remote[0].modelscope.as_mut().unwrap().revision = "master".into();
        assert!(matches!(
            model.validate_source(Source::ModelScope),
            Err(DownloadError::Identity)
        ));
    }

    #[test]
    fn unavailable_source_does_not_replace_partial_data_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.gguf");
        fs::write(&path, &data()[..1234]).unwrap();
        assert!(matches!(
            fetch(&path, "missing", None).0,
            Err(DownloadError::Network(_))
        ));
        assert_eq!(fs::metadata(&path).unwrap().len(), 1234);
        fetch(&path, "", None).0.unwrap();
    }
}
