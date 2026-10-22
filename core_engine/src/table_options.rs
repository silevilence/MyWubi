//! 码表类型与临时模式配置；路径和快捷键校验可由各前端复用。
use crate::{config::Hotkey, dictionary::TableConfig, Dictionary};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 码表检索语义。旧码表默认沿用形码规则。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableType {
    #[default]
    Shape,
    Pinyin,
}

/// 主码表声明的一次性辅助输入功能。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TemporaryConfig {
    /// 空字符串禁用；否则是一个非编码字符。
    pub key: String,
    pub pinyin_table: String,
    pub english_table: String,
    pub reverse_lookup: bool,
}

impl TemporaryConfig {
    /// 只加载直接引用，不解释被引用码表的 temporary 配置。
    pub fn load_table(
        main_table: &Path,
        reference: &str,
        pinyin: bool,
    ) -> Result<Option<Arc<Dictionary>>, String> {
        let Some(path) = Self::resolve_path(main_table, reference) else {
            return if pinyin {
                Err("临时拼音未配置码表".into())
            } else {
                Ok(None)
            };
        };
        let dict = Dictionary::load(&path).map_err(|error| error.to_string())?;
        if pinyin && dict.table_config().table_type != TableType::Pinyin {
            return Err(format!("{} 必须声明 type: pinyin", path.display()));
        }
        Ok(Some(dict))
    }
    pub fn trigger(&self) -> Option<char> {
        let mut chars = self.key.chars();
        let first = chars.next()?;
        chars.next().is_none().then_some(first)
    }

    pub fn resolve_path(main_table: &Path, reference: &str) -> Option<PathBuf> {
        if reference.is_empty() {
            return None;
        }
        let path = Path::new(reference);
        Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            main_table
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(path)
        })
    }
}

/// 保存和运行时加载均调用；不依赖平台虚拟键码。
pub fn validate_temporary_hotkeys(config: &TableConfig, hotkey: &Hotkey) -> Result<(), String> {
    let Some(trigger) = config.temporary.trigger() else {
        return Ok(());
    };
    for (field, name) in [
        ("page_next", &hotkey.page_next),
        ("page_prev", &hotkey.page_prev),
        ("select_second", &hotkey.select_second),
        ("select_third", &hotkey.select_third),
    ] {
        let character = match name.as_str() {
            "comma" => Some(','),
            "period" => Some('.'),
            "semicolon" => Some(';'),
            "quote" => Some('\''),
            "minus" => Some('-'),
            "equal" => Some('='),
            "space" => Some(' '),
            _ => None,
        };
        if character == Some(trigger) {
            return Err(format!("temporary.key `{trigger}` 与 hotkey.{field} 冲突"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_follow_the_main_table_directory() {
        assert_eq!(
            TemporaryConfig::resolve_path(Path::new("bundle/main.dict"), "pinyin.dict"),
            Some(PathBuf::from("bundle/pinyin.dict"))
        );
        assert_eq!(
            TemporaryConfig::resolve_path(Path::new("main.dict"), ""),
            None
        );
    }

    #[test]
    fn entry_key_conflicts_name_the_hotkey() {
        let mut config = TableConfig::default();
        config.temporary.key = "'".into();
        assert!(validate_temporary_hotkeys(&config, &Hotkey::default())
            .unwrap_err()
            .contains("select_third"));
        config.temporary.key = "`".into();
        assert!(validate_temporary_hotkeys(&config, &Hotkey::default()).is_ok());
    }

    #[test]
    fn pinyin_reference_requires_a_typed_table() {
        let path =
            std::env::temp_dir().join(format!("mywubi-reference-{}.dict", std::process::id()));
        std::fs::write(&path, "a\t工\t1\n").unwrap();
        let error = TemporaryConfig::load_table(&path, path.to_str().unwrap(), true).unwrap_err();
        std::fs::remove_file(&path).unwrap();
        assert!(error.contains("type: pinyin"));
        assert!(TemporaryConfig::load_table(&path, "", false)
            .unwrap()
            .is_none());
        assert!(TemporaryConfig::load_table(&path, "", true).is_err());
    }
}
