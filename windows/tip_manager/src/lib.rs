//! Windows TIP（Text Input Processor）管理器。
//!
//! 封装 TIP 全生命周期管理：注册表写入 + `ITfInputProcessorProfileMgr` COM 调用。

pub mod detect;
pub mod error;
pub mod guids;
pub mod profile;
pub mod registrar;

pub use error::TipManagerError;

/// TIP 当前安装与启用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipStatus {
    /// 系统中未找到本 TIP 的注册痕迹。
    NotInstalled,
    /// TIP 已注册但当前被禁用。
    InstalledDisabled,
    /// TIP 已注册且已启用。
    InstalledEnabled,
    /// 检测到异常状态（注册表残缺、COM 不可用等）。
    Unknown,
}

pub fn install(dll_path: &str) -> Result<(), TipManagerError> {
    todo!("Task 6")
}

pub fn uninstall() -> Result<(), TipManagerError> {
    todo!("Task 6")
}

pub fn enable() -> Result<(), TipManagerError> {
    todo!("Task 6")
}

pub fn disable() -> Result<(), TipManagerError> {
    todo!("Task 6")
}

pub fn detect_status() -> TipStatus {
    todo!("Task 5")
}
