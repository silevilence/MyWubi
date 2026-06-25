//! 配置程序全局状态容器。

use core_engine::Config;
use std::path::PathBuf;
use std::sync::mpsc;

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
}

/// 后台线程中进行的文件选择请求。
#[derive(Debug)]
pub struct PickRequest {
    pub target: FilePickTarget,
    pub rx: mpsc::Receiver<Option<PathBuf>>,
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
    /// 当前浏览的码表目录（从 system_table 父目录解析）。
    pub table_dir: PathBuf,
    /// 码表目录下扫描到的 .dict 文件名列表。
    pub scanned_tables: Vec<String>,
    /// TIP 当前安装与启用状态（启动时检测一次）。
    pub tip_status: tip_manager::TipStatus,
    /// 卸载确认状态：true 表示已点击一次卸载，等待二次确认。
    pub uninstall_confirm: bool,
    /// 是否正在执行 TIP 操作（显示 spinner）。
    pub tip_operation_pending: bool,
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
        let table_dir = config.dictionary.system_table
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let scanned_tables = scan_table_dir(&table_dir);
        Self {
            config,
            dirty: false,
            active_panel: Panel::Basic,
            config_path,
            status_msg: None,
            portable,
            load_error,
            pending_pick: None,
            table_dir,
            scanned_tables,
            tip_status: tip_manager::detect_status(),
            uninstall_confirm: false,
            tip_operation_pending: false,
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
            self.status_msg = Some(format!("[ERR] 覆盖失败: {e}"));
            log::error!("覆盖损坏配置失败: {e}");
        } else {
            self.config = cfg;
            self.load_error = None;
            self.status_msg = Some("[OK] 已用默认配置覆盖".into());
            log::info!("已用默认配置覆盖损坏的 {}", self.config_path.display());
        }
    }

    /// 重新扫描码表目录，更新 scanned_tables。
    /// 若当前 system_table 文件不在新目录中，自动选第一个 .dict。
    pub fn rescan_tables(&mut self) {
        self.scanned_tables = scan_table_dir(&self.table_dir);
        if !self.scanned_tables.is_empty() {
            let current = self.config.dictionary.system_table
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("");
            if !self.scanned_tables.iter().any(|t| t == current) {
                if let Some(first) = self.scanned_tables.first() {
                    self.config.dictionary.system_table = self.table_dir.join(first);
                }
            }
        }
    }
}

/// 扫描目录下所有 .dict 文件，返回排序后的文件名列表（不含路径）。
fn scan_table_dir(dir: &std::path::Path) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "dict"))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    files.sort();
    files
}