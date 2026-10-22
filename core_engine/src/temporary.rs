//! 一次性输入使用的只读辅助码表。文件加载在会话外完成。
use crate::Dictionary;
use std::sync::Arc;

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
    pub pinyin: Option<Arc<Dictionary>>,
    pub english: Option<Arc<Dictionary>>,
    pub pinyin_error: Option<String>,
    pub english_error: Option<String>,
}
