//! 文件日志初始化：输出到软件目录下 `log/settings.log`。

use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};
use std::fs;
use std::path::PathBuf;

/// 初始化日志系统。返回日志文件路径（用于状态显示）。
///
/// 日志级别可通过 `RUST_LOG` 环境变量控制（`error`/`warn`/`info`/`debug`/`trace`），
/// 默认 `info`。
pub fn init() -> Option<PathBuf> {
    let log_dir = log_dir()?;
    fs::create_dir_all(&log_dir).ok()?;

    let log_path = log_dir.join("settings.log");
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build();

    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);

    let file = fs::File::create(&log_path).ok()?;
    WriteLogger::init(level, config, file).ok()?;
    Some(log_path)
}

/// 计算日志目录：exe 目录下 `log/`。
fn log_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    Some(exe_dir.join("log"))
}
