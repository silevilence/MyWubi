//! 配置程序全局状态容器。

use core_engine::Config;
use std::path::PathBuf;

/// 当前激活的面板。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Basic,
    Appearance,
    Dictionary,
    About,
}

/// 应用状态。所有面板通过 `&mut AppState` 读写。
pub struct AppState {
    /// 当前配置（内存中的工作副本）。
    pub config: Config,
    /// 是否有未保存的改动。
    pub dirty: bool,
    /// 当前激活面板。
    pub active_panel: Panel,
    /// 配置文件实际路径。
    pub config_path: PathBuf,
    /// 状态栏消息（如"已保存"、错误信息）。
    pub status_msg: Option<String>,
    /// 是否便携模式。
    pub portable: bool,
}

impl AppState {
    /// 从磁盘加载配置并构造状态。若配置损坏则用默认配置覆盖。
    pub fn load(config_path: PathBuf) -> Self {
        let portable = crate::config_path::is_portable();
        let config = match Config::load(&config_path) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::warn!("配置加载失败，使用默认配置: {e}");
                let cfg = Config::default();
                let _ = cfg.save(&config_path);
                cfg
            }
        };
        Self {
            config,
            dirty: false,
            active_panel: Panel::Basic,
            config_path,
            status_msg: None,
            portable,
        }
    }

    /// 标记为已修改。
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}