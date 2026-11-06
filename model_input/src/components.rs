//! Explicit component installation. Uses the model downloader's bounded resumable transport,
//! but trusts only the application's compiled catalog, never model-supplied executable data.
use crate::{download, package::Artifact};
use runtime_components::{self as runtime, Component};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub fn capability(backend: crate::package::Backend) -> &'static str {
    match backend {
        crate::package::Backend::LlamaCpp => "llama",
        crate::package::Backend::OnnxRuntime => "onnx",
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Progress {
    pub phase: String,
    pub completed: u64,
    pub total: u64,
    pub message: String,
}

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("已取消，下载保留，可重试续传".into())
    } else {
        Ok(())
    }
}

fn check_space(required: u64, available: u64) -> Result<(), String> {
    if available < required { Err(format!("磁盘空间不足：需要 {required} 字节，可用 {available} 字节")) } else { Ok(()) }
}

/// Only a completely verified stage becomes visible at its immutable version path.
fn unpack(
    root: &Path,
    component: &Component,
    archive: &Path,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    runtime::verify_file(archive, &component.archive).map_err(|e| e.to_string())?;
    let stage = root
        .join(".downloads")
        .join(format!("{}.stage", component.key()));
    if stage.exists() {
        runtime::no_links(&stage).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() != component.files.len() {
        return Err("组件压缩包包含未声明文件".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for index in 0..zip.len() {
        cancelled(cancel)?;
        let mut input = zip.by_index(index).map_err(|e| e.to_string())?;
        let expected = component
            .files
            .iter()
            .find(|f| f.path == input.name())
            .ok_or("组件含未声明路径")?;
        if !runtime::safe_path(input.name())
            || !seen.insert(input.name().to_ascii_lowercase())
            || input.is_dir()
            || input.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
            || input.size() != expected.bytes
        {
            return Err("组件压缩包路径或大小无效".into());
        }
        let path = stage.join(&expected.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            runtime::no_links(parent).map_err(|e| e.to_string())?;
        }
        if path.exists() {
            runtime::no_links(&path).map_err(|e| e.to_string())?;
        }
        let mut output = fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut copied = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            cancelled(cancel)?;
            let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            if copied > expected.bytes {
                return Err("组件解压超过声明大小".into());
            }
            output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        }
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        runtime::verify_file(&path, expected).map_err(|e| e.to_string())?;
    }
    cancelled(cancel)?;
    // Stage and destination are on the same volume. Never overwrite a working version.
    runtime::verify_directory(&stage, component).map_err(|e| e.to_string())?;
    drop(runtime::lock(&stage, ".lease", false).map_err(|e| e.to_string())?);
    let destination = root.join(component.key());
    if destination.exists() {
        // A corrupt same-version install must be repairable. Keep it for diagnosis;
        // a running service still blocks repair through its lease.
        let lease = runtime::lock(&destination, ".lease", true).map_err(|e| e.to_string())?;
        let quarantine = root.join(format!(
            "{}.invalid-{}",
            component.key(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        fs::rename(&destination, quarantine).map_err(|e| e.to_string())?;
        drop(lease);
    }
    fs::rename(&stage, &destination).map_err(|e| e.to_string())?;
    runtime::verify(root, component).map_err(|e| e.to_string())
}

pub fn install(
    config: &Path,
    capability: &str,
    cancel: &AtomicBool,
    mut publish: impl FnMut(Progress),
) -> Result<(), String> {
    let catalog = runtime::catalog().map_err(|e| e.to_string())?;
    let plan = runtime::plan(
        &catalog,
        capability,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
    .map_err(|e| e.to_string())?;
    install_plan(&runtime::store(config), &plan, cancel, &mut publish)
}

fn install_plan(
    root: &Path,
    plan: &[&Component],
    cancel: &AtomicBool,
    publish: &mut impl FnMut(Progress),
) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    runtime::no_links(root).map_err(|e| e.to_string())?;
    let lock = runtime::lock(root, "install.lock", true).map_err(|e| e.to_string())?;
    let downloads = root.join(".downloads");
    fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    runtime::no_links(&downloads).map_err(|e| e.to_string())?;
    let pending: Vec<_> = plan
        .iter()
        .filter(|c| runtime::verify(root, c).is_err())
        .collect();
    let required = pending
        .iter()
        .map(|c| c.archive.bytes + c.installed_bytes)
        .sum::<u64>()
        .saturating_add(16 * 1024 * 1024);
    let available = fs2::available_space(root).map_err(|e| e.to_string())?;
    check_space(required, available)?;
    let total = pending.iter().map(|c| c.archive.bytes).sum();
    let mut completed = 0;
    for component in pending {
        cancelled(cancel)?;
        let path = downloads.join(format!(
            "{}-{}.part",
            component.key(),
            component.archive.sha256
        ));
        if path.exists() {
            runtime::no_links(&path).map_err(|e| e.to_string())?;
        }
        let artifact = Artifact {
            path: component.archive.path.clone(),
            bytes: component.archive.bytes,
            sha256: component.archive.sha256.clone(),
        };
        download::transfer(
            &download::agent(),
            &component.source,
            &path,
            &artifact,
            cancel,
            &mut |n| {
                publish(Progress {
                    phase: "downloading".into(),
                    completed: completed + n,
                    total,
                    message: component.id.clone(),
                })
            },
        )
        .map_err(|e| e.to_string())?;
        publish(Progress {
            phase: "installing".into(),
            completed: completed + component.archive.bytes,
            total,
            message: "正在校验并安装组件".into(),
        });
        unpack(root, component, &path, cancel)?;
        completed += component.archive.bytes;
    }
    drop(lock);
    publish(Progress {
        phase: "ready".into(),
        completed,
        total,
        message: "组件已安装；模型选择及预测开关未改变".into(),
    });
    Ok(())
}

/// Call after the UI has named affected models. Saved selections and runtime leases block removal.
pub fn remove(config: &Path, key: &str) -> Result<(), String> {
    let catalog = runtime::catalog().map_err(|e| e.to_string())?;
    let component = catalog.iter().find(|c| c.key() == key).ok_or("未知组件")?;
    let saved = core_engine::Config::load(config).map_err(|e| e.to_string())?;
    if saved.prediction.model.is_some() && !component.capabilities.is_empty() {
        return Err("已保存的模型可能使用此组件；请先选择词典预测并保存".into());
    }
    let root = runtime::store(config);
    let _install = runtime::lock(&root, "install.lock", true).map_err(|e| e.to_string())?;
    for dependent in &catalog {
        if dependent.dependencies.contains(&component.key()) && root.join(dependent.key()).exists()
        {
            return Err("已安装组件仍依赖此共享组件".into());
        }
    }
    let directory = root.join(component.key());
    let lease = runtime::lock(&directory, ".lease", true).map_err(|e| e.to_string())?;
    runtime::verify(&root, component).map_err(|e| e.to_string())?;
    // Renaming while holding the lease makes the component unavailable to new launches.
    let removed = root.join(format!("{}.removed", component.key()));
    fs::rename(&directory, &removed).map_err(|e| e.to_string())?;
    drop(lease);
    runtime::no_links(&removed).map_err(|e| e.to_string())?;
    fs::remove_dir_all(removed).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn insufficient_space_is_rejected_before_download_or_activation() {
        assert!(check_space(1024, 1023).unwrap_err().contains("空间不足"));
        assert!(check_space(1024, 1024).is_ok());
    }
    fn fixture(root: &Path, version: &str) -> Component {
        let bytes = b"test executable fixture";
        let artifact = runtime::File {
            path: "model-worker.exe".into(),
            bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        };
        let archive = root.join(format!("{version}.zip"));
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.start_file("model-worker.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
        zip.finish().unwrap();
        Component {
            id: "cpu".into(),
            version: version.into(),
            application: "=0.7.0".into(),
            os: "windows".into(),
            architecture: "x86_64".into(),
            capabilities: vec!["llama".into(), "onnx".into()],
            dependencies: vec![],
            source: format!(
                "https://github.com/silevilence/MyWubi/releases/download/v0.7.0/{version}.zip"
            ),
            license: "test".into(),
            archive: runtime::File {
                path: format!("{version}.zip"),
                bytes: fs::metadata(&archive).unwrap().len(),
                sha256: format!("{:x}", Sha256::digest(fs::read(&archive).unwrap())),
            },
            installed_bytes: artifact.bytes,
            files: vec![artifact],
        }
    }
    #[test]
    fn interrupted_install_and_corrupt_update_preserve_working_version() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("components");
        fs::create_dir_all(root.join(".downloads")).unwrap();
        let old = fixture(dir.path(), "1");
        let new = fixture(dir.path(), "2");
        unpack(
            &root,
            &old,
            &dir.path().join("1.zip"),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(unpack(
            &root,
            &new,
            &dir.path().join("2.zip"),
            &AtomicBool::new(true)
        )
        .is_err());
        assert!(!root.join(new.key()).exists());
        fs::write(dir.path().join("2.zip"), b"corrupt").unwrap();
        assert!(unpack(
            &root,
            &new,
            &dir.path().join("2.zip"),
            &AtomicBool::new(false)
        )
        .is_err());
        runtime::verify(&root, &old).unwrap();
        let new = fixture(dir.path(), "2");
        unpack(
            &root,
            &new,
            &dir.path().join("2.zip"),
            &AtomicBool::new(false),
        )
        .unwrap();
        runtime::verify(&root, &new).unwrap();
        runtime::verify(&root, &old).unwrap();
    }
    #[test]
    fn verified_cache_installs_offline_and_keeps_configuration_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("components");
        let c = fixture(dir.path(), "1");
        fs::create_dir_all(root.join(".downloads")).unwrap();
        let partial =
            root.join(".downloads")
                .join(format!("{}-{}.part", c.key(), c.archive.sha256));
        fs::copy(dir.path().join("1.zip"), partial).unwrap();
        let config = dir.path().join("config.toml");
        fs::write(&config, "[prediction]\nenabled = false\n").unwrap();
        let before = fs::read(&config).unwrap();
        install_plan(&root, &[&c], &AtomicBool::new(false), &mut |_| {}).unwrap();
        runtime::verify(&root, &c).unwrap();
        assert_eq!(fs::read(config).unwrap(), before);
        let lease = runtime::lock(&root.join(c.key()), ".lease", false).unwrap();
        assert!(runtime::lock(&root.join(c.key()), ".lease", true).is_err());
        drop(lease);
        assert!(runtime::lock(&root.join(c.key()), ".lease", true).is_ok());
    }
    #[test]
    fn unlisted_native_file_in_partial_stage_blocks_activation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("components");
        let c = fixture(dir.path(), "1");
        let stage = root.join(".downloads").join(format!("{}.stage", c.key()));
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("unlisted.dll"), b"untrusted").unwrap();
        assert!(unpack(
            &root,
            &c,
            &dir.path().join("1.zip"),
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(!root.join(c.key()).exists());
    }
}
