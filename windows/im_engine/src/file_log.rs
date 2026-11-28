//! im_engine.dll 文件日志初始化。
//!
//! 宿主可能长时间持有日志句柄。安装版将日志放在 Velopack 替换的目录之外。
//! 在 DllMain loader lock 下不可初始化——由 `dll_log_init()` 惰性初始化，
//! 在 `DllGetClassObject` 或 `im_engine_init` 等安全时机调用。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// 日志文件写入器（全局单例）。
struct FileLogger {
    file: Mutex<File>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = record.level();
        let target = record.target();
        let msg = record.args();
        // 使用系统时间戳，避免依赖 chrono
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        if let Ok(mut file) = self.file.lock() {
            // Multiple host processes append to this file. Format first so one
            // append contains the whole record instead of interleaved fragments.
            let line = format!("[{ts:.3}] [{level}] [{target}] {msg}\n");
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

fn log_file_path(dll_dir: &Path) -> PathBuf {
    // Velopack moves current into packages/VelopackTemp across parent directories.
    // Even a share-delete log handle in a TSF host can prevent that move.
    let install_root = dll_dir.parent().filter(|_| {
        dll_dir
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("current"))
            && dll_dir.join("sq.version").is_file()
    });
    install_root
        .unwrap_or(dll_dir)
        .join("log")
        .join("im_engine.log")
}

/// 惰性初始化文件日志。可以安全地多次调用（仅第一次生效）。
pub fn init() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let log_path = match crate::dll_directory() {
            Some(directory) => log_file_path(Path::new(&directory)),
            None => {
                eprintln!("[im_engine_log] 无法确定日志路径");
                return;
            }
        };
        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let file = match OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(&log_path)
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[im_engine_log] 无法打开日志文件 {log_path:?}: {e}");
                return;
            }
        };

        let level = std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| s.parse::<log::LevelFilter>().ok())
            .unwrap_or(log::LevelFilter::Info);

        log::set_max_level(level);

        let logger = Box::new(FileLogger {
            file: Mutex::new(file),
        });
        match log::set_boxed_logger(logger) {
            Ok(()) => {
                log::info!("[im_engine_log] 日志已初始化: {}", log_path.display());
            }
            Err(e) => {
                eprintln!("[im_engine_log] set_boxed_logger 失败: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_engine_log_allows_velopack_to_move_current_into_its_backup() {
        let install = tempfile::tempdir().unwrap();
        let current = install.path().join("current");
        let backup = install.path().join("packages/VelopackTemp");
        fs::create_dir(&current).unwrap();
        fs::create_dir_all(&backup).unwrap();
        assert_eq!(log_file_path(&current), current.join("log/im_engine.log"));
        File::create(current.join("sq.version")).unwrap();
        let path = log_file_path(&current);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut log = File::create(&path).unwrap();
        fs::rename(&current, backup.join("old")).unwrap();
        log.write_all(b"still running").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"still running");
    }
}
