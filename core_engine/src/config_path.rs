use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfigPath {
    pub path: PathBuf,
    pub portable: bool,
    pub fallback_message: Option<String>,
}

#[derive(Debug, Error)]
pub enum PathError {
    #[error("无法获取 exe 路径: {0}")]
    ExePath(String),
    #[error("无法获取 AppData 路径")]
    AppData,
    #[error("无法创建配置目录 {0}: {1}")]
    CreateDir(PathBuf, String),
    #[error("无法写入默认配置 {0}: {1}")]
    WriteConfig(PathBuf, String),
}

pub fn resolve_config_path() -> Result<ResolvedConfigPath, PathError> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| PathError::ExePath(e.to_string()))?
        .parent()
        .ok_or_else(|| PathError::ExePath("exe 无父目录".into()))?
        .to_path_buf();
    let app_dir = dirs::config_dir().ok_or(PathError::AppData)?.join("MyWubi");
    resolve_config_path_from(&exe_dir, &app_dir)
}

pub fn resolve_config_path_from(
    exe_dir: &Path,
    app_config_dir: &Path,
) -> Result<ResolvedConfigPath, PathError> {
    let portable_path = exe_dir.join("config.toml");
    if portable_path.exists() {
        return Ok(ResolvedConfigPath {
            path: portable_path,
            portable: true,
            fallback_message: None,
        });
    }

    if let Err(err) = std::fs::create_dir_all(app_config_dir) {
        ensure_default_config(&portable_path)?;
        log::warn!(
            "无法创建 AppData 目录 ({}), 回退到便携配置 {}",
            err,
            portable_path.display()
        );
        return Ok(ResolvedConfigPath {
            path: portable_path,
            portable: true,
            fallback_message: Some("[!] 已切换便携模式（AppData 不可用）".into()),
        });
    }

    let config_path = app_config_dir.join("config.toml");
    let created = ensure_default_config(&config_path)?;
    if created {
        copy_default_tables(exe_dir, app_config_dir)?;
    }
    Ok(ResolvedConfigPath {
        path: config_path,
        portable: false,
        fallback_message: None,
    })
}

pub fn resolve_resource_path(config_path: &Path, resource_path: &Path) -> PathBuf {
    if resource_path.is_absolute() {
        return resource_path.to_path_buf();
    }

    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(resource_path)
}

pub fn is_portable_mode(exe_dir: &Path) -> bool {
    exe_dir.join("config.toml").exists()
}

fn ensure_default_config(config_path: &Path) -> Result<bool, PathError> {
    if config_path.exists() {
        return Ok(false);
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PathError::CreateDir(parent.to_path_buf(), e.to_string()))?;
    }

    crate::Config::default()
        .save(config_path)
        .map_err(|e| PathError::WriteConfig(config_path.to_path_buf(), e.to_string()))?;
    Ok(true)
}

fn copy_default_tables(exe_dir: &Path, app_config_dir: &Path) -> Result<(), PathError> {
    let src_dir = exe_dir.join("tables");
    if !src_dir.is_dir() {
        return Ok(());
    }

    let dst_dir = app_config_dir.join("tables");
    std::fs::create_dir_all(&dst_dir)
        .map_err(|e| PathError::CreateDir(dst_dir.clone(), e.to_string()))?;

    let entries = std::fs::read_dir(&src_dir)
        .map_err(|e| PathError::CreateDir(src_dir.clone(), e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "dict") {
            let Some(file_name) = path.file_name() else { continue; };
            let _ = std::fs::copy(&path, dst_dir.join(file_name));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("mywubi-{name}-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn portable_mode_wins_when_exe_dir_has_config() {
        let root = unique_temp_dir("portable");
        let exe_dir = root.join("portable-bin");
        let app_dir = root.join("appdata").join("MyWubi");
        std::fs::create_dir_all(&exe_dir).unwrap();
        Config::default().save(exe_dir.join("config.toml")).unwrap();

        let resolved = resolve_config_path_from(&exe_dir, &app_dir).unwrap();

        assert_eq!(resolved.path, exe_dir.join("config.toml"));
        assert!(resolved.portable);
        assert!(resolved.fallback_message.is_none());
    }

    #[test]
    fn appdata_mode_creates_default_config_when_portable_missing() {
        let root = unique_temp_dir("appdata");
        let exe_dir = root.join("bin");
        let app_dir = root.join("appdata").join("MyWubi");
        std::fs::create_dir_all(&exe_dir).unwrap();

        let resolved = resolve_config_path_from(&exe_dir, &app_dir).unwrap();

        assert_eq!(resolved.path, app_dir.join("config.toml"));
        assert!(!resolved.portable);
        assert!(resolved.path.exists());

        let loaded = Config::load(&resolved.path).unwrap();
        assert_eq!(loaded.basic.candidate_count, Config::default().basic.candidate_count);
    }

    #[test]
    fn relative_resource_paths_are_resolved_from_config_parent() {
        let config_path = PathBuf::from(r"C:\Users\test\AppData\Roaming\MyWubi\config.toml");
        let resolved = resolve_resource_path(&config_path, Path::new("tables/wubi86.dict"));

        assert_eq!(
            resolved,
            PathBuf::from(r"C:\Users\test\AppData\Roaming\MyWubi\tables\wubi86.dict")
        );
    }
}
