//! 一次性输入使用的只读辅助码表。文件加载在会话外完成。
use crate::Dictionary;
use std::collections::BTreeMap;
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
}
