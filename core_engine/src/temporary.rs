//! 一次性输入使用的只读辅助码表。文件加载在会话外完成。
use crate::{config::Hotkey, validate_temporary_hotkeys, Dictionary, TemporaryConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 仅由主码表文件构建，不包含合并后的用户词库。
#[derive(Debug, Default, Clone)]
pub struct ReverseLookup(Arc<ReverseCodes>);

#[derive(Debug, Default)]
struct ReverseCodes {
    // Words are sorted, and each offset starts its contiguous slice of codes.
    words: Vec<(String, usize)>,
    codes: Vec<String>,
}

impl ReverseLookup {
    pub fn new(main: &Dictionary) -> Self {
        main.reverse.get_or_init(|| Self::build(main)).clone()
    }

    fn build(main: &Dictionary) -> Self {
        let mut entries: Vec<_> = main.entries().iter().collect();
        entries.sort_unstable_by(|a, b| {
            a.word
                .cmp(&b.word)
                .then_with(|| a.code.chars().count().cmp(&b.code.chars().count()))
                .then_with(|| a.code.cmp(&b.code))
        });
        entries.dedup_by(|a, b| a.word == b.word && a.code == b.code);
        let mut index = ReverseCodes::default();
        for group in entries.chunk_by(|a, b| a.word == b.word) {
            index.words.push((group[0].word.clone(), index.codes.len()));
            index
                .codes
                .extend(group.iter().map(|entry| entry.code.clone()));
        }
        Self(Arc::new(index))
    }

    pub fn codes(&self, word: &str) -> &[String] {
        let index = &self.0;
        let Ok(i) = index
            .words
            .binary_search_by(|(key, _)| key.as_str().cmp(word))
        else {
            return &[];
        };
        let end = index
            .words
            .get(i + 1)
            .map_or(index.codes.len(), |next| next.1);
        &index.codes[index.words[i].1..end]
    }

    pub fn hint(&self, word: &str) -> String {
        let codes = self.codes(word);
        if codes.is_empty() {
            String::new()
        } else {
            codes.join(" / ")
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InputMode {
    #[default]
    Main,
    Menu,
    Pinyin,
    English,
    Emoji,
    Kaomoji,
    Kana,
    Math,
    Punctuation,
    Symbols,
    Greek,
    Cyrillic,
    Latin,
    Phonetic,
    Enclosed,
    Numbers,
    Arrows,
    Currency,
    Units,
    Shapes,
    BoxDrawing,
    Braille,
    Music,
    Diacritics,
}

impl InputMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "",
            Self::Menu => "临时模式",
            Self::Pinyin => "临时拼音",
            Self::English => "临时英文",
            Self::Emoji => "Emoji",
            Self::Kaomoji => "颜文字",
            Self::Kana => "假名",
            Self::Math => "数学符号",
            Self::Punctuation => "标点",
            Self::Symbols => "其他符号",
            Self::Greek => "希腊字母",
            Self::Cyrillic => "西里尔字母",
            Self::Latin => "拼音与拉丁",
            Self::Phonetic => "音标",
            Self::Enclosed => "圈号与括号",
            Self::Numbers => "数字形式",
            Self::Arrows => "箭头",
            Self::Currency => "货币",
            Self::Units => "单位",
            Self::Shapes => "几何图形",
            Self::BoxDrawing => "制表与方块",
            Self::Braille => "盲文",
            Self::Music => "音乐符号",
            Self::Diacritics => "组合附加符",
        }
    }

    pub fn expression_kind(self) -> Option<crate::expressions::ExpressionKind> {
        use crate::expressions::ExpressionKind;
        match self {
            Self::Emoji => Some(ExpressionKind::Emoji),
            Self::Kaomoji => Some(ExpressionKind::Kaomoji),
            Self::Kana => Some(ExpressionKind::Kana),
            Self::Math => Some(ExpressionKind::Math),
            Self::Punctuation => Some(ExpressionKind::Punctuation),
            Self::Symbols => Some(ExpressionKind::Symbols),
            Self::Greek => Some(ExpressionKind::Greek),
            Self::Cyrillic => Some(ExpressionKind::Cyrillic),
            Self::Latin => Some(ExpressionKind::Latin),
            Self::Phonetic => Some(ExpressionKind::Phonetic),
            Self::Enclosed => Some(ExpressionKind::Enclosed),
            Self::Numbers => Some(ExpressionKind::Numbers),
            Self::Arrows => Some(ExpressionKind::Arrows),
            Self::Currency => Some(ExpressionKind::Currency),
            Self::Units => Some(ExpressionKind::Units),
            Self::Shapes => Some(ExpressionKind::Shapes),
            Self::BoxDrawing => Some(ExpressionKind::BoxDrawing),
            Self::Braille => Some(ExpressionKind::Braille),
            Self::Music => Some(ExpressionKind::Music),
            Self::Diacritics => Some(ExpressionKind::Diacritics),

            _ => None,
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
