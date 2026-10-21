//! 本输入法 TIP（Text Input Processor）跨进程复用的 COM 标识符。
//!
//! 常量定义已迁移至 `tip_manager::guids`，本文件仅做 re-export 以保持
//! im_engine 内部 `crate::guids` 引用不中断。

pub use tip_manager::guids::*;

/// 中英文切换保留键。
pub(crate) const GUID_PRESERVED_SHIFT: windows::core::GUID =
    windows::core::GUID::from_u128(0x1a2b3c4d_5e6f_7a8b_9c0d_1e2f3a4b5c6d);
