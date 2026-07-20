//! `config.toml` 配置文件解析与序列化。
//!
//! 采用 `serde` + `toml` 双端统一格式，Windows settings.exe 与 Android
//! Compose 配置界面均读写同一份结构。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// 配置读取/写入过程中可能出现的错误。
#[derive(Debug, Error)]
pub enum Error {
    #[error("无法读取配置文件 {0}: {1}")]
    Io(PathBuf, String),
    #[error("配置文件格式错误: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("配置项 `{0}` 非法: {1}")]
    Invalid(String, String),
}

/// 全局配置根。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub basic: Basic,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub dictionary: DictionaryCfg,
    #[serde(default)]
    pub hotkey: Hotkey,
}

/// 基础行为配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Basic {
    /// 候选词显示个数 (1..=10)。
    #[serde(default = "default_candidate_count")]
    pub candidate_count: u8,
    /// 上屏方式。
    #[serde(default)]
    pub commit_mode: CommitMode,
    /// 中英文切换键。
    #[serde(default = "default_switch_key")]
    pub switch_key: SwitchKey,
    /// 四码唯一时自动上屏。
    #[serde(default = "default_true")]
    pub auto_commit_unique: bool,
    /// 超过码表最大码长时，已有候选则自动上屏首选并开始下一轮编码。
    #[serde(default = "default_true")]
    pub commit_on_max_code_overflow: bool,
    /// 候选编码不全时显示后续编码提示。
    #[serde(default = "default_true")]
    pub show_code_hints: bool,
    /// 标点输入处理策略。
    #[serde(default)]
    pub punctuation_mode: PunctuationMode,
}

/// 上屏方式。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommitMode {
    /// 空格首选上屏。
    #[default]
    #[serde(rename = "space_first")]
    SpaceFirst,
    /// 回车上屏。
    #[serde(rename = "enter_commit")]
    EnterCommit,
}

/// 中英文切换键。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SwitchKey {
    #[default]
    #[serde(rename = "shift")]
    Shift,
    #[serde(rename = "caps_lock")]
    CapsLock,
    #[serde(rename = "ctrl_space")]
    CtrlSpace,
}

/// 标点输入处理策略。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum PunctuationMode {
    /// 标点先加入原始输入缓冲，最后与编码一起按原样上屏。
    #[default]
    #[serde(rename = "buffered_commit")]
    BufferedCommit,
    /// 标点立即透传给应用，不加入当前编码缓冲。
    #[serde(rename = "direct_commit")]
    DirectCommit,
}

/// 外观样式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appearance {
    /// 候选框字体大小 (pt)。
    #[serde(default = "default_font_size")]
    pub font_size: u16,
    /// 主色 (ARGB)。
    #[serde(default = "default_primary")]
    pub primary_color: u32,
    /// 背景色 (ARGB)。
    #[serde(default = "default_background")]
    pub background_color: u32,
    /// 候选项高亮色 (ARGB)。
    #[serde(default = "default_highlight")]
    pub highlight_color: u32,
}

/// 码表/词库配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryCfg {
    /// 系统码表文件路径（相对配置文件或绝对路径）。
    #[serde(default = "default_system_table")]
    pub system_table: PathBuf,
    /// 用户词库文件路径（可读写）。
    #[serde(default = "default_user_table")]
    pub user_table: PathBuf,
    /// 启用精确匹配优先。
    #[serde(default = "default_true")]
    pub enable_exact_match: bool,
    /// 启用模糊音（预留给音码输入法）。
    #[serde(default)]
    pub enable_fuzzy: bool,
    /// 启用用户词库功能（词库数据在 config 外单独保存）。
    #[serde(default)]
    pub enable_user_dict: bool,
}

/// 快捷键配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotkey {
    /// 下一页键。
    #[serde(default = "default_page_next")]
    pub page_next: String,
    /// 上一页键。
    #[serde(default = "default_page_prev")]
    pub page_prev: String,
    /// 快速选择第二候选。
    #[serde(default = "default_select_second")]
    pub select_second: String,
    /// 快速选择第三候选。
    #[serde(default = "default_select_third")]
    pub select_third: String,
    /// 二三级简码切换。
    #[serde(default = "default_toggle_simplify")]
    pub toggle_simplify: String,
}

impl Default for Basic {
    fn default() -> Self {
        Self {
            candidate_count: default_candidate_count(),
            commit_mode: CommitMode::SpaceFirst,
            switch_key: default_switch_key(),
            auto_commit_unique: true,
            commit_on_max_code_overflow: true,
            show_code_hints: true,
            punctuation_mode: PunctuationMode::BufferedCommit,
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            primary_color: default_primary(),
            background_color: default_background(),
            highlight_color: default_highlight(),
        }
    }
}

impl Default for DictionaryCfg {
    fn default() -> Self {
        Self {
            system_table: default_system_table(),
            user_table: default_user_table(),
            enable_exact_match: true,
            enable_fuzzy: false,
            enable_user_dict: false,
        }
    }
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            page_next: default_page_next(),
            page_prev: default_page_prev(),
            select_second: default_select_second(),
            select_third: default_select_third(),
            toggle_simplify: default_toggle_simplify(),
        }
    }
}

fn default_candidate_count() -> u8 {
    5
}
fn default_true() -> bool {
    true
}
fn default_switch_key() -> SwitchKey {
    SwitchKey::Shift
}
fn default_font_size() -> u16 {
    14
}
fn default_primary() -> u32 {
    0xFF1E88E5
}
fn default_background() -> u32 {
    0xFFFFFFFF
}
fn default_highlight() -> u32 {
    0xFFFFD54F
}
fn default_system_table() -> PathBuf {
    PathBuf::from("tables/wubi86.dict")
}
fn default_user_table() -> PathBuf {
    PathBuf::from("tables/user.dict")
}
fn default_page_next() -> String {
    "comma".into()
}
fn default_page_prev() -> String {
    "period".into()
}
fn default_select_second() -> String {
    "semicolon".into()
}
fn default_select_third() -> String {
    "quote".into()
}
fn default_toggle_simplify() -> String {
    "ctrl_shift_s".into()
}

impl Config {
    /// 从 TOML 字符串解析。
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Result<Self, Error> {
        let cfg: Config = toml::from_str(text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// 从文件加载。
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Io(path.to_path_buf(), e.to_string()))?;
        Self::from_str(&text)
    }

    /// 序列化为 TOML 字符串。
    pub fn to_string_toml(&self) -> Result<String, Error> {
        toml::to_string_pretty(self).map_err(|e| Error::Invalid("serialize".into(), e.to_string()))
    }

    /// 安全写入：先写入临时文件再原子性改名，避免写入中断损坏原配置。
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let path = path.as_ref();
        self.validate()?;
        self.validate_hotkey_conflicts()?;
        let text = self.to_string_toml()?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text.as_bytes()).map_err(|e| Error::Io(tmp.clone(), e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| Error::Io(path.to_path_buf(), e.to_string()))?;
        Ok(())
    }

    /// 语义校验。
    fn validate(&self) -> Result<(), Error> {
        if self.basic.candidate_count == 0 || self.basic.candidate_count > 10 {
            return Err(Error::Invalid(
                "basic.candidate_count".into(),
                "应位于 1..=10".into(),
            ));
        }
        if self.appearance.font_size == 0 {
            return Err(Error::Invalid(
                "appearance.font_size".into(),
                "不能为 0".into(),
            ));
        }
        Ok(())
    }

    fn validate_hotkey_conflicts(&self) -> Result<(), Error> {
        let bindings = [
            ("hotkey.page_next", self.hotkey.page_next.as_str()),
            ("hotkey.page_prev", self.hotkey.page_prev.as_str()),
            ("hotkey.select_second", self.hotkey.select_second.as_str()),
            ("hotkey.select_third", self.hotkey.select_third.as_str()),
        ];

        for (index, (field, key)) in bindings.iter().enumerate() {
            for (other_field, other_key) in bindings.iter().skip(index + 1) {
                if key == other_key {
                    return Err(Error::Invalid(
                        (*other_field).into(),
                        format!("与 `{field}` 冲突：都绑定到 `{key}`"),
                    ));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[basic]
candidate_count = 7
commit_mode = "enter_commit"
switch_key = "ctrl_space"
auto_commit_unique = false
commit_on_max_code_overflow = false
show_code_hints = false
punctuation_mode = "direct_commit"

[appearance]
font_size = 16
primary_color = 0xFF000000
background_color = 0xFFFFFFFF
highlight_color = 0xFFFFD54F

[dictionary]
system_table = "tables/wubi86.dict"
user_table = "tables/user.dict"
enable_exact_match = true
enable_fuzzy = true
enable_user_dict = true

[hotkey]
page_next = "comma"
page_prev = "period"
select_second = "semicolon"
select_third = "quote"
toggle_simplify = "ctrl_shift_s"
"#;

    #[test]
    fn parse_full_config() {
        let cfg = Config::from_str(SAMPLE).unwrap();
        assert_eq!(cfg.basic.candidate_count, 7);
        assert_eq!(cfg.basic.commit_mode, CommitMode::EnterCommit);
        assert_eq!(cfg.basic.switch_key, SwitchKey::CtrlSpace);
        assert!(!cfg.basic.auto_commit_unique);
        assert!(!cfg.basic.commit_on_max_code_overflow);
        assert!(!cfg.basic.show_code_hints);
        assert_eq!(cfg.basic.punctuation_mode, PunctuationMode::DirectCommit);
        assert_eq!(cfg.appearance.font_size, 16);
        assert!(cfg.dictionary.enable_fuzzy);
        assert!(cfg.dictionary.enable_user_dict);
    }

    #[test]
    fn parse_with_defaults() {
        let cfg = Config::from_str("[basic]\ncandidate_count = 3\n").unwrap();
        assert_eq!(cfg.basic.candidate_count, 3);
        assert_eq!(cfg.basic.commit_mode, CommitMode::SpaceFirst);
        assert!(cfg.basic.commit_on_max_code_overflow);
        assert!(cfg.basic.show_code_hints);
        assert_eq!(cfg.basic.punctuation_mode, PunctuationMode::BufferedCommit);
        assert_eq!(cfg.appearance.font_size, 14);
        assert!(!cfg.dictionary.enable_user_dict);
    }

    #[test]
    fn rejects_invalid_candidate_count() {
        let err = Config::from_str("[basic]\ncandidate_count = 0\n").unwrap_err();
        assert!(matches!(err, Error::Invalid(k, _) if k == "basic.candidate_count"));
    }

    #[test]
    fn roundtrip_toml() {
        let cfg = Config::from_str(SAMPLE).unwrap();
        let s = cfg.to_string_toml().unwrap();
        let cfg2 = Config::from_str(&s).unwrap();
        assert_eq!(cfg2.basic.candidate_count, 7);
        assert_eq!(cfg2.basic.commit_mode, CommitMode::EnterCommit);
    }

    #[test]
    fn quick_select_hotkeys_roundtrip_and_default() {
        let cfg = Config::from_str(SAMPLE).unwrap();
        let serialized = cfg.to_string_toml().unwrap();
        assert!(serialized.contains("select_second = \"semicolon\""));
        assert!(serialized.contains("select_third = \"quote\""));

        let defaulted = Config::from_str("[basic]\ncandidate_count = 3\n").unwrap();
        let default_text = defaulted.to_string_toml().unwrap();
        assert!(default_text.contains("select_second = \"semicolon\""));
        assert!(default_text.contains("select_third = \"quote\""));
    }
}
