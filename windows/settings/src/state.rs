//! 配置程序全局状态容器。

use std::path::PathBuf;
use std::sync::mpsc;

use crate::table_session::TableSession;
use core_engine::{Config, UserDictionary};

/// 当前激活的面板。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Basic,
    Appearance,
    Dictionary,
    TipManager,
    About,
}

/// 文件选择对话框目标字段。
#[derive(Debug, Clone, Copy)]
pub enum FilePickTarget {
    SystemTableDir,
    UserTable,
    UserDictionaryImport,
    UserDictionaryExport,
}

/// 后台线程中进行的文件选择请求。
#[derive(Debug)]
pub struct PickRequest {
    pub target: FilePickTarget,
    pub rx: mpsc::Receiver<Option<PathBuf>>,
}

pub struct UserDictionaryEditor {
    pub open: bool,
    pub dictionary: Option<UserDictionary>,
    pub code: String,
    pub word: String,
    pub weight: u32,
    pub selected: Option<usize>,
}

impl Default for UserDictionaryEditor {
    fn default() -> Self {
        Self {
            open: false,
            dictionary: None,
            code: String::new(),
            word: String::new(),
            weight: 1000,
            selected: None,
        }
    }
}

impl UserDictionaryEditor {
    pub fn clear_form(&mut self) {
        self.code.clear();
        self.word.clear();
        self.weight = 1000;
        self.selected = None;
    }
}

/// 无法通过 `mark_dirty` 的 panel 子函数（因双重借用限制）使用此函数
/// 直接设置 dirty 并清空 status_msg，保持与 `mark_dirty` 语义一致。
pub(crate) fn set_dirty(dirty: &mut bool, status_msg: &mut Option<String>) {
    *dirty = true;
    *status_msg = None;
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
    /// 后台 rfd 文件选择请求（避免阻塞 UI）。
    pub pending_pick: Option<PickRequest>,

    /// TIP 当前安装与启用状态（启动时检测一次）。
    pub tip_status: tip_manager::TipStatus,
    /// 卸载确认状态：true 表示已点击一次卸载，等待二次确认。
    pub uninstall_confirm: bool,
    /// 是否正在执行 TIP 操作（显示 spinner）。
    pub tip_operation_pending: bool,
    /// Velopack 按需更新状态机（关于面板「检查更新」交互）。
    pub update_state: crate::vpk::UpdateState,
    /// 后台更新 worker 句柄（检查/下载进行中时存在）。
    pub update_worker: Option<crate::vpk::UpdateWorker>,
    /// 用户词库管理窗口状态。
    pub user_dictionary_editor: UserDictionaryEditor,
    /// 当前系统码表的编辑状态。
    pub tables: TableSession,
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
    pub fn load(config_path: PathBuf, portable: bool) -> Self {
        let (config, load_error) = match Config::load(&config_path) {
            Ok(cfg) => (cfg, None),
            Err(e) => {
                log::warn!("配置加载失败，暂用默认配置（未覆盖原文件）: {e}");
                (
                    Config::default(),
                    Some(LoadError {
                        message: e.to_string(),
                        path: config_path.clone(),
                    }),
                )
            }
        };
        let resolved_system_table = crate::config_path::resolve_resource_path(
            &config_path,
            &config.dictionary.system_table,
        );
        let tables = TableSession::load(resolved_system_table, &config.dictionary.user_table);
        let access_error = load_error
            .is_none()
            .then(|| crate::app_container::prepare(&config_path, &config).err())
            .flatten();
        if let Some(error) = &access_error {
            log::warn!("{error}");
        }
        Self {
            config,
            dirty: false,
            active_panel: Panel::Basic,
            config_path,
            status_msg: access_error,
            portable,
            load_error,
            pending_pick: None,
            tip_status: tip_manager::detect_status(),
            uninstall_confirm: false,
            tip_operation_pending: false,
            update_state: crate::vpk::UpdateState::Idle,
            update_worker: None,
            user_dictionary_editor: UserDictionaryEditor::default(),
            tables,
        }
    }

    /// 标记为已修改，并清空状态栏消息（新改动使旧消息失效）。
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.status_msg = None;
    }

    pub(crate) fn prepare_app_container_access(&mut self) {
        if let Err(error) = crate::app_container::prepare(&self.config_path, &self.config) {
            log::warn!("{error}");
            self.status_msg = Some(format!("[!] {error}"));
        }
    }

    /// 用户确认用默认配置覆盖损坏文件后调用。
    pub fn apply_default_overwrite(&mut self) {
        let cfg = Config::default();
        if let Err(e) = cfg.save(&self.config_path) {
            self.status_msg = Some(format!("[ERR] 覆盖失败: {e}"));
            log::error!("覆盖损坏配置失败: {e}");
        } else {
            self.config = cfg;
            self.load_error = None;
            self.status_msg = Some("[OK] 已用默认配置覆盖".into());
            log::info!("已用默认配置覆盖损坏的 {}", self.config_path.display());
            self.prepare_app_container_access();
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.dirty || self.tables.is_dirty()
    }

    pub fn browse_tables(&mut self, directory: PathBuf) {
        match self.tables.browse(directory, &mut self.config) {
            Ok(()) => self.mark_dirty(),
            Err(error) => self.status_msg = Some(format!("[ERR] {error}")),
        }
    }

    pub fn select_table(&mut self, path: PathBuf) {
        match self.tables.select(path, &mut self.config) {
            Ok(()) => self.mark_dirty(),
            Err(error) => self.status_msg = Some(format!("[ERR] {error}")),
        }
    }

    pub fn rescan_tables(&mut self) {
        self.tables.rescan(&self.config.dictionary.user_table);
    }
}
