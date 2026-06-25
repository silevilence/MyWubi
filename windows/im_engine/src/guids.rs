//! 本输入法 TIP（Text Input Processor）跨进程复用的 COM 标识符。
//!
//! 常量定义已迁移至 `tip_manager::guids`，本文件仅做 re-export 以保持
//! im_engine 内部 `crate::guids` 引用不中断。

pub use tip_manager::guids::*;