//! TIP 管理器错误类型。

use std::path::PathBuf;

/// TIP 管理操作可能失败的所有方式。
#[derive(Debug, thiserror::Error)]
pub enum TipManagerError {
    /// 注册表操作失败。
    #[error("注册表操作失败: {0}")]
    Registry(String),

    /// COM 初始化或调用失败。
    #[error("COM 调用失败: {0}")]
    Com(String),

    /// 找不到 im_engine.dll。
    #[error("找不到 DLL: {0}")]
    DllNotFound(PathBuf),

    /// 权限不足（非管理员运行）。
    #[error("需要管理员权限")]
    AccessDenied,

    /// TIP 状态不一致（例如注册表有但 COM 找不到）。
    #[error("TIP 状态不一致: {0}")]
    InconsistentState(String),
}
