//! 配置文件路径定位：复用 `core_engine` 的共享规则。

use std::path::{Path, PathBuf};

pub use core_engine::config_path::PathError;

/// 解析配置文件路径。
pub fn resolve_config_path() -> Result<(PathBuf, bool, Option<String>), PathError> {
    let resolved = core_engine::config_path::resolve_config_path()?;
    Ok((resolved.path, resolved.portable, resolved.fallback_message))
}

/// 判断当前 exe 目录是否处于便携模式。
pub fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(core_engine::config_path::is_portable_mode))
        .unwrap_or(false)
}

/// 将资源路径按配置文件所在目录解析为绝对路径。
pub fn resolve_resource_path(config_path: &Path, resource_path: &Path) -> PathBuf {
    core_engine::config_path::resolve_resource_path(config_path, resource_path)
}
