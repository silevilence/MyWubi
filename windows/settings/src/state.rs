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
    /// 启动期配置加载错误（若有）。UI 据此弹错误对话框让用户选择处理方式。
    pub load_error: Option<LoadError>,
}

/// 配置加载失败信息。用户需在 UI 中确认后才覆盖损坏文件。
#[derive(Debug, Clone)]
pub struct LoadError {
    /// 错误详情。
    pub message: String,
    /// 配置文件路径（用于"打开文件位置"）。
    pub path: PathBuf,
}

impl AppState {
    /// 从磁盘加载配置并构造状态。
    ///
    /// 若配置损坏，**不立即覆盖**——而是记录 `load_error`，由 UI 弹对话框
    /// 让用户选择"加载默认配置"（此时才覆盖）或"打开文件位置"自行修复。
    pub fn load(config_path: PathBuf) -> Self {
        let portable = crate::config_path::is_portable();
        let (config, load_error) = match Config::load(&config_path) {
            Ok(cfg) => (cfg, None),
            Err(e) => {
                log::warn!("配置加载失败，暂用默认配置（未覆盖原文件）: {e}");
                (Config::default(), Some(LoadError {
                    message: e.to_string(),
                    path: config_path.clone(),
                }))
            }
        };
        Self {
            config,
            dirty: false,
            active_panel: Panel::Basic,
            config_path,
            status_msg: None,
            portable,
            load_error,
        }
    }

    /// 标记为已修改，并清空状态栏消息（新改动使旧消息失效）。
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.status_msg = None;
    }

    /// 用户确认用默认配置覆盖损坏文件后调用。
    pub fn apply_default_overwrite(&mut self) {
        let cfg = Config::default();
        if let Err(e) = cfg.save(&self.config_path) {
            self.status_msg = Some(format!("❌ 覆盖失败: {e}"));
            log::error!("覆盖损坏配置失败: {e}");
        } else {
            self.config = cfg;
            self.load_error = None;
            self.status_msg = Some("✅ 已用默认配置覆盖".into());
            log::info!("已用默认配置覆盖损坏的 {}", self.config_path.display());
        }
    }
}