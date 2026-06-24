//! 配置文件路径定位：exe 同目录优先（便携模式），回退 `%APPDATA%\MyWubi\`。

use std::path::PathBuf;
use thiserror::Error;

/// 路径定位错误。
#[derive(Debug, Error)]
pub enum PathError {
    #[error("无法获取 exe 路径: {0}")]
    ExePath(String),
    #[error("无法获取 AppData 路径")]
    AppData,
    #[error("无法创建配置目录 {0}: {1}")]
    CreateDir(PathBuf, String),
}

/// 解析配置文件路径。
///
/// 1. 若 exe 同目录存在 `config.toml` → 返回该路径（便携模式）
/// 2. 否则回退到 `%APPDATA%\MyWubi\config.toml`，必要时创建目录与默认配置
/// 3. 若 AppData 目录创建失败 → 回退到 exe 同目录
pub fn resolve_config_path() -> Result<PathBuf, PathError> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| PathError::ExePath(e.to_string()))?
        .parent()
        .ok_or_else(|| PathError::ExePath("exe 无父目录".into()))?
        .to_path_buf();

    let portable = exe_dir.join("config.toml");
    if portable.exists() {
        return Ok(portable);
    }

    let appdata = dirs::config_dir()
        .ok_or(PathError::AppData)?
        .join("MyWubi");
    let cfg_path = appdata.join("config.toml");

    if !appdata.exists() {
        if let Err(e) = std::fs::create_dir_all(&appdata) {
            // 回退便携模式
            log::warn!("无法创建 AppData 目录 ({}), 回退便携模式", e);
            return Ok(portable);
        }
    }

    if !cfg_path.exists() {
        // 从 exe 同目录的默认模板复制，或写入内置默认
        let template = exe_dir.join("config.toml");
        if template.exists() {
            let _ = std::fs::copy(&template, &cfg_path);
        } else {
            let cfg = core_engine::Config::default();
            let _ = cfg.save(&cfg_path);
        }
    }

    Ok(cfg_path)
}

/// 判断当前是否便携模式（exe 同目录有 config.toml）。
pub fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.toml")))
        .map(|p| p.exists())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn portable_mode_when_exe_dir_has_config() {
        let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        let portable = exe_dir.join("config.toml");
        let existed = portable.exists();
        if !existed {
            fs::write(&portable, "# test placeholder\n").unwrap();
        }
        let path = resolve_config_path().unwrap();
        assert_eq!(path, portable);
        if !existed {
            fs::remove_file(&portable).ok();
        }
    }

    #[test]
    fn is_portable_reflects_exe_dir_config() {
        let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        let portable = exe_dir.join("config.toml");
        let existed = portable.exists();
        if !existed {
            fs::write(&portable, "# test\n").unwrap();
        }
        assert!(is_portable());
        if !existed {
            fs::remove_file(&portable).ok();
        }
    }
}
