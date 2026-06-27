//! im_engine.dll 文件日志初始化。
//!
//! 使用 `log` crate 的日志门面，将日志写入 DLL 同级目录的 `log/im_engine.log`。
//! 在 DllMain loader lock 下不可初始化——由 `dll_log_init()` 惰性初始化，
//! 在 `DllGetClassObject` 或 `im_engine_init` 等安全时机调用。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
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
            let _ = writeln!(file, "[{ts:.3}] [{level}] [{target}] {msg}");
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// 计算日志文件路径：DLL 所在目录下的 `log/im_engine.log`。
fn log_file_path() -> Option<PathBuf> {
    let handle = crate::module_handle();
    if handle == 0 {
        return None;
    }
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
    let mut buf = vec![0u16; 260];
    let hmodule = HMODULE(handle as *mut core::ffi::c_void);
    let len = unsafe { GetModuleFileNameW(Some(hmodule), &mut buf) as usize };
    if len == 0 {
        return None;
    }
    let dll_path = String::from_utf16_lossy(&buf[..len]);
    let dll_dir = std::path::Path::new(&dll_path).parent()?;
    Some(dll_dir.join("log").join("im_engine.log"))
}

/// 惰性初始化文件日志。可以安全地多次调用（仅第一次生效）。
pub fn init() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let log_path = match log_file_path() {
            Some(p) => p,
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
