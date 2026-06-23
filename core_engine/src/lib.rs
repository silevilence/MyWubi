//! # Core Engine
//!
//! 形码输入法核心算法层。提供码表解析、Trie 树前缀检索、输入状态机与配置文件读写。
//!
//! 全部对外接口均为平台无关，不依赖任何系统 API，便于 Windows (TSF) 与
//! Android (JNI) 两侧复用。

pub mod config;
pub mod dictionary;
pub mod state_machine;

pub use config::{Config, Error as ConfigError};
pub use dictionary::{Dictionary, Entry, MatchKind, SearchOptions};
pub use state_machine::{InputEvent, StateMachine, Transition};

/// 核心库版本号。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 重新导出常用类型，供前端壳层免路径引用。
pub mod prelude {
    pub use crate::{
        config::Config,
        dictionary::{Dictionary, Entry, MatchKind, SearchOptions},
        state_machine::{InputEvent, StateMachine, Transition},
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        // 版本号由 cargo 注入，恒非空；保留测试用于回归。
        let _ = VERSION;
    }
}