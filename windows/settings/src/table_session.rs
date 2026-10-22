//! 码表编辑会话：集中草稿、选择与持久化结果。
use core_engine::{
    read_table, save_table, validate_table, Config, Entry, TableConfig, TableValidationIssue,
    TableValidationReport,
};
use std::cmp::Reverse;
use std::path::{Path, PathBuf};

pub struct TableEditor {
    pub open: bool,
    path: PathBuf,
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
    validation_stale: bool,
    dirty: bool,
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
    pub fn set_table_type(&mut self, table_type: core_engine::TableType) {
        self.config.table_type = table_type;
        if table_type == core_engine::TableType::Pinyin {
            self.config.charset = "abcdefghijklmnopqrstuvwxyz".into();
            self.wildcard_key.clear();
            self.config.wildcard_key = None;
        }
        self.mark_changed();
        self.refresh_validation();
    }

    pub fn validate_context(&self, hotkey: &core_engine::config::Hotkey) -> Result<(), String> {
        let config = self.draft_config()?;
        core_engine::validate_temporary_hotkeys(&config, hotkey)?;
        validate_references(&config, &self.path)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn validation_stale(&self) -> bool {
        self.validation_stale
    }
    pub fn mark_changed(&mut self) {
        self.dirty = true;
        self.validation_stale = true;
    }

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
            max_code_len: self.config.max_code_len,
            ..self.config.clone()
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
        let mut config = self.draft_config()?;
        // 跨目录另存保留原引用的目标；转换为绝对路径后再校验，成功才更新草稿。
        if path.parent() != self.path.parent() {
            for reference in [
                &mut config.temporary.pinyin_table,
                &mut config.temporary.english_table,
            ] {
                if !reference.is_empty() && Path::new(reference).is_relative() {
                    if let Some(target) =
                        core_engine::TemporaryConfig::resolve_path(&self.path, reference)
                    {
                        *reference = std::path::absolute(target)
                            .map_err(|e| e.to_string())?
                            .to_string_lossy()
                            .into_owned();
                    }
                }
            }
        }
        validate_references(&config, &path)?;
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

fn validate_references(config: &TableConfig, path: &Path) -> Result<(), String> {
    for (pinyin, reference) in [
        (true, &config.temporary.pinyin_table),
        (false, &config.temporary.english_table),
    ] {
        if !reference.is_empty() {
            core_engine::TemporaryConfig::load_table(path, reference, pinyin)?;
        }
    }
    Ok(())
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

/// UI 的草稿字段留在 editor 中，文件选择和保存协议由会话拥有。
pub struct TableSession {
    directory: PathBuf,
    tables: Vec<String>,
    pub editor: TableEditor,
}

#[derive(Debug)]
pub struct SaveFailure {
    pub table_saved: bool,
    pub message: String,
}

impl std::fmt::Display for SaveFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.table_saved {
            write!(f, "码表已保存，但配置保存失败：{}", self.message)
        } else {
            write!(f, "保存失败：{}", self.message)
        }
    }
}

impl TableSession {
    pub fn load(path: PathBuf, excluded: &Path) -> Self {
        let directory = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let tables = scan_table_dir(&directory, Some(excluded));
        Self {
            directory,
            tables,
            editor: TableEditor::load(path),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn tables(&self) -> &[String] {
        &self.tables
    }
    pub fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }

    fn ensure_saved(&self) -> Result<(), String> {
        if self.is_dirty() {
            Err("请先保存当前码表改动".into())
        } else {
            Ok(())
        }
    }

    /// 在替换任何选择状态之前检查草稿及新文件；失败保留原会话。
    pub fn select(&mut self, path: PathBuf, config: &mut Config) -> Result<(), String> {
        self.ensure_saved()?;
        let editor = TableEditor::load(path.clone());
        if let Some(error) = &editor.load_error {
            return Err(error.clone());
        }
        self.directory = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        self.tables = scan_table_dir(&self.directory, Some(&config.dictionary.user_table));
        self.editor = editor;
        config.dictionary.system_table = path;
        Ok(())
    }

    pub fn browse(&mut self, directory: PathBuf, config: &mut Config) -> Result<(), String> {
        self.ensure_saved()?;
        let tables = scan_table_dir(&directory, Some(&config.dictionary.user_table));
        let current = self.editor.path.file_name().and_then(|name| name.to_str());
        let selected = current
            .filter(|name| tables.iter().any(|item| item == name))
            .or_else(|| tables.first().map(String::as_str));
        if let Some(selected) = selected {
            self.select(directory.join(selected), config)
        } else {
            self.directory = directory;
            self.tables = tables;
            Ok(())
        }
    }

    /// 重扫只刷新目录列表，绝不替换草稿或当前选择。
    pub fn rescan(&mut self, excluded: &Path) {
        self.tables = scan_table_dir(&self.directory, Some(excluded));
    }

    /// 初始化完成后显式加载当前目录；普通重扫继续保留草稿。
    pub fn reload_directory(&mut self, config: &mut Config) -> Result<(), String> {
        self.browse(self.directory.clone(), config)
    }

    pub fn save_as(&mut self, name: &str, config: &mut Config) -> Result<PathBuf, String> {
        self.editor.validate_context(&config.hotkey)?;
        let name = name.trim();
        if name.is_empty() {
            return Err("请输入自定义码表名称".into());
        }
        if !matches!(
            Path::new(name).components().next(),
            Some(std::path::Component::Normal(_))
        ) || Path::new(name).components().count() != 1
        {
            return Err("码表名称不能包含路径".into());
        }
        let name = if name.ends_with(".dict") {
            name.to_string()
        } else {
            format!("{name}.dict")
        };
        let path = self.directory.join(name);
        if path.exists() {
            return Err(format!("{} 已存在", path.display()));
        }
        self.editor.save_to(path.clone())?;
        config.dictionary.system_table = path.clone();
        self.rescan(&config.dictionary.user_table);
        Ok(path)
    }

    /// 配置预检在所有写入之前；文件各自原子写，失败明确报告已保存部分。
    pub fn save(&mut self, config: &Config, config_path: &Path) -> Result<(), SaveFailure> {
        let failure = |message: String| SaveFailure {
            table_saved: false,
            message,
        };
        config
            .validate_for_save()
            .map_err(|error| failure(error.to_string()))?;
        self.editor
            .validate_context(&config.hotkey)
            .map_err(failure)?;
        let table_saved = self.is_dirty();
        if table_saved {
            self.editor
                .save_to(self.editor.path.clone())
                .map_err(failure)?;
        }
        config.save(config_path).map_err(|error| SaveFailure {
            table_saved,
            message: error.to_string(),
        })
    }
}

/// 扫描目录下所有 .dict 文件，返回排序后的文件名列表（不含路径）。
fn scan_table_dir(dir: &std::path::Path, excluded: Option<&std::path::Path>) -> Vec<String> {
    let excluded_name = excluded
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str());
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "dict"))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| excluded_name != Some(name.as_str()))
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fields_survive_edit_save_reload_and_cross_directory_save() {
        let root = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let (mut session, mut config) = session(root.path());
        std::fs::write(
            root.path().join("py.dict"),
            "---\ntype: pinyin\nmax_code_len: 64\n---\nni'hao\t你好\n",
        )
        .unwrap();
        session.editor.config.temporary = core_engine::TemporaryConfig {
            key: "`".into(),
            pinyin_table: "py.dict".into(),
            english_table: String::new(),
            reverse_lookup: true,
        };
        session.editor.mark_changed();
        session
            .save(&config, &root.path().join("config.toml"))
            .unwrap();
        let loaded = TableEditor::load(session.editor.path().to_path_buf());
        assert_eq!(loaded.config.temporary, session.editor.config.temporary);
        let target = destination.path().join("moved.dict");
        session.editor.save_to(target.clone()).unwrap();
        let moved = TableEditor::load(target);
        assert_eq!(
            Path::new(&moved.config.temporary.pinyin_table),
            root.path().join("py.dict")
        );
        assert!(moved.validate_context(&config.hotkey).is_ok());
        config.hotkey.select_third = "quote".into();
        session.editor.config.temporary.key = "'".into();
        session.editor.mark_changed();
        assert!(session
            .save(&config, &root.path().join("config.toml"))
            .unwrap_err()
            .message
            .contains("select_third"));
        assert!(session.editor.is_dirty());
    }

    #[test]
    fn type_switch_keeps_entries_and_invalid_references_keep_drafts() {
        let root = tempfile::tempdir().unwrap();
        let (mut session, config) = session(root.path());
        let before = session.editor.entries.clone();
        session.editor.wildcard_key = "z".into();
        session
            .editor
            .set_table_type(core_engine::TableType::Pinyin);
        assert_eq!(session.editor.entries, before);
        assert!(session.editor.wildcard_key.is_empty());
        session.editor.config.temporary.key = "`".into();
        session.editor.config.temporary.pinyin_table = "missing.dict".into();
        assert!(session
            .save(&config, &root.path().join("config.toml"))
            .is_err());
        assert!(session.editor.is_dirty());
        assert_eq!(session.editor.config.temporary.pinyin_table, "missing.dict");
    }

    #[test]
    fn pinyin_editor_preserves_apostrophes_and_new_header_preview() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("py.dict");
        std::fs::write(
            &path,
            "---\ntype: pinyin\nmax_code_len: 64\n---\nzhong'guo'ren\t中国人\t5\n",
        )
        .unwrap();
        let mut editor = TableEditor::load(path.clone());
        editor.entries[0].weight = 10;
        editor.mark_changed();
        editor.save_to(path.clone()).unwrap();
        let loaded = TableEditor::load(path);
        assert_eq!(loaded.entries[0].code, "zhong'guo'ren");
        assert!(loaded
            .config
            .yaml_header()
            .unwrap()
            .contains("type: pinyin"));
        assert_eq!(loaded.entries[0].weight, 10);
    }

    fn session(dir: &Path) -> (TableSession, Config) {
        let path = dir.join("original.dict");
        save_table(
            &path,
            &TableConfig::default(),
            &[Entry {
                code: "a".into(),
                word: "工".into(),
                weight: 1,
            }],
        )
        .unwrap();
        let mut config = Config::default();
        config.dictionary.system_table = path.clone();
        (
            TableSession::load(path, &config.dictionary.user_table),
            config,
        )
    }

    fn edit(session: &mut TableSession) {
        session.editor.select(0);
        session.editor.word = "改".into();
        session.editor.update_selected().unwrap();
    }

    #[test]
    fn invalid_config_does_not_write_dirty_table() {
        let dir = tempfile::tempdir().unwrap();
        let (mut session, mut config) = session(dir.path());
        let original = std::fs::read(session.editor.path()).unwrap();
        edit(&mut session);
        config.hotkey.page_prev = config.hotkey.page_next.clone();
        let failure = session
            .save(&config, &dir.path().join("config.toml"))
            .unwrap_err();
        assert!(!failure.table_saved);
        assert!(session.is_dirty());
        assert_eq!(std::fs::read(session.editor.path()).unwrap(), original);
    }

    #[test]
    fn config_write_failure_reports_partial_success_and_can_retry() {
        let dir = tempfile::tempdir().unwrap();
        let (mut session, config) = session(dir.path());
        edit(&mut session);
        let path = dir.path().join("missing/config.toml");
        let failure = session.save(&config, &path).unwrap_err();
        assert!(failure.table_saved);
        assert!(!session.is_dirty());
        assert_eq!(read_table(session.editor.path()).unwrap().1[0].word, "改");
        std::fs::create_dir(path.parent().unwrap()).unwrap();
        session.save(&config, &path).unwrap();
        assert_eq!(
            Config::load(path).unwrap().dictionary.system_table,
            config.dictionary.system_table
        );
    }

    #[test]
    fn invalid_table_preserves_both_files_and_draft() {
        let dir = tempfile::tempdir().unwrap();
        let (mut session, config) = session(dir.path());
        let path = dir.path().join("config.toml");
        config.save(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        session.editor.wildcard_key = "zz".into();
        session.editor.mark_changed();
        assert!(!session.save(&config, &path).unwrap_err().table_saved);
        assert!(session.is_dirty());
        assert_eq!(std::fs::read(path).unwrap(), before);
        assert_eq!(read_table(session.editor.path()).unwrap().1[0].word, "工");
    }

    #[test]
    fn dirty_draft_blocks_directory_and_table_switch_but_survives_rescan() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let (mut session, mut config) = session(dir.path());
        let (_, other_config) = self::session(other.path());
        edit(&mut session);
        assert!(session
            .browse(other.path().to_path_buf(), &mut config)
            .is_err());
        assert!(session
            .select(other_config.dictionary.system_table, &mut config)
            .is_err());
        session.rescan(&config.dictionary.user_table);
        assert_eq!(session.directory(), dir.path());
        assert_eq!(session.editor.path(), config.dictionary.system_table);
        assert_eq!(session.editor.entries[0].word, "改");
        assert!(session.is_dirty());
    }

    #[test]
    fn save_as_then_rescan_preserves_selection_and_saves_config() {
        let dir = tempfile::tempdir().unwrap();
        let (mut session, mut config) = session(dir.path());
        edit(&mut session);
        let path = session.save_as("custom", &mut config).unwrap();
        session.rescan(&config.dictionary.user_table);
        assert_eq!(session.editor.path(), path);
        assert_eq!(config.dictionary.system_table, path);
        assert!(!session.is_dirty());
        assert!(session.tables().iter().any(|name| name == "custom.dict"));
        assert_eq!(
            read_table(dir.path().join("original.dict")).unwrap().1[0].word,
            "工"
        );
        session
            .save(&config, &dir.path().join("config.toml"))
            .unwrap();
        assert_eq!(read_table(&path).unwrap().1[0].word, "改");
    }

    #[test]
    fn header_change_invalidates_validation_until_checked() {
        let dir = tempfile::tempdir().unwrap();
        let (mut session, _) = session(dir.path());
        session.editor.wildcard_key = "zz".into();
        session.editor.mark_changed();
        assert!(session.editor.validation_stale());
        session.editor.refresh_validation();
        assert!(!session.editor.validation_stale());
        assert!(!session.editor.validation.is_valid());
        assert!(session.is_dirty());
    }

    #[test]
    fn failed_selection_preserves_current_path_and_draft() {
        let dir = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();
        let (mut session, mut config) = session(dir.path());
        let path = config.dictionary.system_table.clone();
        session
            .browse(empty.path().to_path_buf(), &mut config)
            .unwrap();
        assert_eq!(session.directory(), empty.path());
        assert!(session
            .select(dir.path().join("absent.dict"), &mut config)
            .is_err());
        assert_eq!(session.editor.path(), path);
        assert_eq!(config.dictionary.system_table, path);
    }

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

    #[test]
    fn scan_table_dir_hides_configured_user_dictionary() {
        let dir = std::env::temp_dir().join(format!(
            "mywubi-settings-tables-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("wubi86.dict"), "").unwrap();
        std::fs::write(dir.join("user.dict"), "").unwrap();

        let tables = scan_table_dir(&dir, Some(std::path::Path::new("tables/user.dict")));
        let _ = std::fs::remove_dir_all(dir);

        assert_eq!(tables, vec!["wubi86.dict"]);
    }
    #[test]
    fn initialization_loads_first_template_after_missing_table_and_empty_browse() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.dictionary.system_table = dir.path().join("missing.dict");
        let mut session = TableSession::load(
            config.dictionary.system_table.clone(),
            &config.dictionary.user_table,
        );
        assert!(session.editor.load_error.is_some());
        session
            .browse(dir.path().to_path_buf(), &mut config)
            .unwrap();
        let (_, template_config) = self::session(dir.path());
        session.reload_directory(&mut config).unwrap();
        assert!(session.editor.load_error.is_none());
        assert_eq!(session.editor.entries.len(), 1);
        assert_eq!(
            session.editor.path(),
            template_config.dictionary.system_table
        );
        assert_eq!(config.dictionary.system_table, session.editor.path());
        edit(&mut session);
        assert!(session.reload_directory(&mut config).is_err());
        assert_eq!(session.editor.entries[0].word, "改");
    }
}
