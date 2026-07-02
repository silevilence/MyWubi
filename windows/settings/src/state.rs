//! 配置程序全局状态容器。

use std::cmp::Reverse;
use std::path::PathBuf;
use std::sync::mpsc;

use core_engine::{
    read_table, save_table, validate_table, Config, Entry, TableConfig, TableValidationIssue,
    TableValidationReport, UserDictionary,
};

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

pub struct TableEditor {
    pub open: bool,
    pub path: PathBuf,
    pub config: TableConfig,
    pub wildcard_key: String,
    pub entries: Vec<Entry>,
    pub search: String,
    pub filtered_indices: Option<Vec<usize>>,
    pub selected: Option<usize>,
    pub code: String,
    pub word: String,
    pub weight: u32,
    pub save_as_name: String,
    pub validation: TableValidationReport,
    pub validation_stale: bool,
    pub dirty: bool,
    pub load_error: Option<String>,
}

impl Default for TableEditor {
    fn default() -> Self {
        Self {
            open: false,
            path: PathBuf::new(),
            config: TableConfig::default(),
            wildcard_key: String::new(),
            entries: Vec::new(),
            search: String::new(),
            filtered_indices: None,
            selected: None,
            code: String::new(),
            word: String::new(),
            weight: 1,
            save_as_name: String::new(),
            validation: TableValidationReport::default(),
            validation_stale: false,
            dirty: false,
            load_error: None,
        }
    }
}

impl TableEditor {
    pub fn load(path: PathBuf) -> Self {
        let mut editor = Self {
            path: path.clone(),
            ..Self::default()
        };
        match read_table(&path) {
            Ok((config, entries)) => {
                editor.wildcard_key = config
                    .wildcard_key
                    .map(|character| character.to_string())
                    .unwrap_or_default();
                editor.config = config;
                editor.entries = entries;
                editor.refresh_validation();
            }
            Err(error) => editor.load_error = Some(error.to_string()),
        }
        editor
    }

    pub fn draft_config(&self) -> Result<TableConfig, String> {
        let wildcard = self.wildcard_key.trim();
        let wildcard_key = if wildcard.is_empty() {
            None
        } else {
            let mut characters = wildcard.chars();
            let first = characters.next().unwrap_or_default();
            if characters.next().is_some() {
                return Err("wildcard_key 必须为空或单个字符".into());
            }
            Some(first)
        };
        Ok(TableConfig {
            wildcard_key,
            charset: self.config.charset.clone(),
        })
    }

    pub fn refresh_filter(&mut self) {
        let query = self.search.trim();
        self.filtered_indices = if query.is_empty() {
            None
        } else {
            let mut matches: Vec<_> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    search_relevance(entry, query).map(|relevance| (relevance, index))
                })
                .collect();
            matches.sort_by_key(|(relevance, index)| (*relevance, *index));
            Some(matches.into_iter().map(|(_, index)| index).collect())
        };
    }

    pub fn visible_len(&self) -> usize {
        self.filtered_indices
            .as_ref()
            .map_or(self.entries.len(), Vec::len)
    }

    pub fn visible_entry_index(&self, row: usize) -> usize {
        self.filtered_indices
            .as_ref()
            .map_or(row, |indices| indices[row])
    }

    pub fn select(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        self.code.clone_from(&entry.code);
        self.word.clone_from(&entry.word);
        self.weight = entry.weight;
        self.selected = Some(index);
    }

    pub fn update_selected(&mut self) -> Result<(), String> {
        let index = self.selected.ok_or_else(|| "请先选择词条".to_string())?;
        let entry = self
            .entries
            .get_mut(index)
            .ok_or_else(|| "所选词条已不存在".to_string())?;
        entry.code = self.code.trim().to_string();
        entry.word = self.word.trim().to_string();
        entry.weight = self.weight;
        self.dirty = true;
        self.refresh_filter();
        self.refresh_validation();
        Ok(())
    }

    pub fn refresh_validation(&mut self) {
        self.validation = match self.draft_config() {
            Ok(config) => validate_table(&config, &self.entries, 100),
            Err(message) => TableValidationReport {
                issue_count: 1,
                issues: vec![TableValidationIssue {
                    entry_index: None,
                    message,
                }],
            },
        };
        self.validation_stale = false;
    }

    pub fn save_to(&mut self, path: PathBuf) -> Result<(), String> {
        let config = self.draft_config()?;
        save_table(&path, &config, &self.entries).map_err(|error| error.to_string())?;
        self.path = path;
        self.config = config;
        self.validation = TableValidationReport::default();
        self.validation_stale = false;
        self.dirty = false;
        self.load_error = None;
        Ok(())
    }
}

fn search_relevance(entry: &Entry, query: &str) -> Option<(u8, usize, Reverse<u32>)> {
    [entry.code.as_str(), entry.word.as_str()]
        .into_iter()
        .filter_map(|value| {
            let rank = if value == query {
                0
            } else if value.starts_with(query) {
                1
            } else if value.contains(query) {
                2
            } else {
                return None;
            };
            Some((rank, value.chars().count(), Reverse(entry.weight)))
        })
        .min()
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
    /// Velopack 按需更新状态机（关于面板「检查更新」交互）。
    pub update_state: crate::vpk::UpdateState,
    /// 后台更新 worker 句柄（检查/下载进行中时存在）。
    pub update_worker: Option<crate::vpk::UpdateWorker>,
    /// 用户词库管理窗口状态。
    pub user_dictionary_editor: UserDictionaryEditor,
    /// 当前系统码表的编辑状态。
    pub table_editor: TableEditor,
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
        let table_dir = resolved_system_table
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let scanned_tables = scan_table_dir(&table_dir);
        let table_editor = TableEditor::load(resolved_system_table);
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
            update_state: crate::vpk::UpdateState::Idle,
            update_worker: None,
            user_dictionary_editor: UserDictionaryEditor::default(),
            table_editor,
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
        if self.table_editor.dirty {
            return;
        }
        let current = self
            .config
            .dictionary
            .system_table
            .file_name()
            .and_then(|file| file.to_str());
        let selected = current
            .filter(|name| self.scanned_tables.iter().any(|table| table == name))
            .or_else(|| self.scanned_tables.first().map(String::as_str));
        if let Some(selected) = selected {
            let path = self.table_dir.join(selected);
            self.config.dictionary.system_table = path.clone();
            self.table_editor = TableEditor::load(path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_editor_filters_by_code_or_word() {
        let mut editor = TableEditor {
            entries: vec![
                Entry {
                    code: "abcd".into(),
                    word: "目标".into(),
                    weight: 1,
                },
                Entry {
                    code: "wxyz".into(),
                    word: "其他".into(),
                    weight: 1,
                },
            ],
            search: "目标".into(),
            ..TableEditor::default()
        };

        editor.refresh_filter();

        assert_eq!(editor.filtered_indices, Some(vec![0]));
    }

    #[test]
    fn table_editor_ranks_code_exact_then_prefix_then_contains() {
        let mut editor = TableEditor {
            entries: vec![
                Entry {
                    code: "ba".into(),
                    word: "包含".into(),
                    weight: 1,
                },
                Entry {
                    code: "aa".into(),
                    word: "前缀".into(),
                    weight: 1,
                },
                Entry {
                    code: "a".into(),
                    word: "完全".into(),
                    weight: 1,
                },
            ],
            search: "a".into(),
            ..TableEditor::default()
        };

        editor.refresh_filter();

        assert_eq!(editor.filtered_indices, Some(vec![2, 1, 0]));
    }

    #[test]
    fn table_editor_ranks_word_exact_then_prefix_then_contains() {
        let mut editor = TableEditor {
            entries: vec![
                Entry {
                    code: "a".into(),
                    word: "施工".into(),
                    weight: 1,
                },
                Entry {
                    code: "b".into(),
                    word: "工作".into(),
                    weight: 1,
                },
                Entry {
                    code: "c".into(),
                    word: "工".into(),
                    weight: 1,
                },
            ],
            search: "工".into(),
            ..TableEditor::default()
        };

        editor.refresh_filter();

        assert_eq!(editor.filtered_indices, Some(vec![2, 1, 0]));
    }

    #[test]
    fn table_editor_rejects_multi_character_wildcard() {
        let editor = TableEditor {
            wildcard_key: "zz".into(),
            ..TableEditor::default()
        };

        assert!(editor.draft_config().is_err());
    }
}
