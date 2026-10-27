//! 一次性输入使用的只读辅助码表。文件加载在会话外完成。
use crate::{config::Hotkey, validate_temporary_hotkeys, Dictionary, TemporaryConfig};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 仅由主码表文件构建，不包含合并后的用户词库。
#[derive(Debug, Default)]
pub struct ReverseLookup(BTreeMap<String, Vec<String>>);

impl ReverseLookup {
    pub fn new(main: &Dictionary) -> Self {
        let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in main.entries() {
            index
                .entry(entry.word.clone())
                .or_default()
                .push(entry.code.clone());
        }
        for codes in index.values_mut() {
            codes.sort_by(|a, b| a.chars().count().cmp(&b.chars().count()).then(a.cmp(b)));
            codes.dedup();
        }
        Self(index)
    }

    pub fn codes(&self, word: &str) -> &[String] {
        self.0.get(word).map_or(&[], Vec::as_slice)
    }

    pub fn hint(&self, word: &str) -> String {
        let codes = self.codes(word);
        if codes.is_empty() {
            String::new()
        } else {
            format!("主码: {}", codes.join(" / "))
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InputMode {
    #[default]
    Main,
    Menu,
    Pinyin,
    English,
}

impl InputMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "",
            Self::Menu => "临时模式",
            Self::Pinyin => "临时拼音",
            Self::English => "临时英文",
        }
    }
}

#[derive(Debug, Default)]
pub struct TemporaryTables {
    pub reverse: Option<ReverseLookup>,
    pub pinyin: Option<Arc<Dictionary>>,
    pub english: Option<Arc<Dictionary>>,
    pub pinyin_error: Option<String>,
    pub english_error: Option<String>,
    pub paths: Vec<PathBuf>,
    /// 加载失败时启动可降级；热重载必须保留旧快照。
    pub load_errors: Vec<String>,
}

impl TemporaryTables {
    pub fn load(main: &Dictionary, main_path: &Path, hotkey: &Hotkey) -> Self {
        Self::load_for_profile(main, main_path, hotkey, crate::BehaviorProfile::LegacyV1)
    }

    /// Capability loading for touch clients does not require a desktop entry key.
    pub fn load_for_profile(
        main: &Dictionary,
        main_path: &Path,
        hotkey: &Hotkey,
        profile: crate::BehaviorProfile,
    ) -> Self {
        let options = &main.table_config().temporary;
        if profile == crate::BehaviorProfile::LegacyV1 && options.trigger().is_none() {
            return Self::default();
        }
        let mut tables = Self {
            reverse: options.reverse_lookup.then(|| ReverseLookup::new(main)),
            paths: [&options.pinyin_table, &options.english_table]
                .into_iter()
                .filter_map(|reference| TemporaryConfig::resolve_path(main_path, reference))
                .collect(),
            ..Default::default()
        };
        if let Err(error) = if profile == crate::BehaviorProfile::LegacyV1 {
            validate_temporary_hotkeys(main.table_config(), hotkey)
        } else {
            Ok(())
        } {
            tables.pinyin_error = Some(error.clone());
            tables.english_error = Some(error.clone());
            tables.load_errors.push(error);
            return tables;
        }
        for (pinyin, reference) in [
            (true, &options.pinyin_table),
            (false, &options.english_table),
        ] {
            match TemporaryConfig::load_table(main_path, reference, pinyin) {
                Ok(dict) => {
                    if pinyin {
                        tables.pinyin = dict;
                    } else {
                        tables.english = dict;
                    }
                }
                Err(error) => {
                    if !reference.is_empty() {
                        tables.load_errors.push(error.clone());
                    }
                    if pinyin {
                        tables.pinyin_error = Some(error);
                    } else {
                        tables.english_error = Some(error);
                    }
                }
            }
        }
        tables
    }
}
