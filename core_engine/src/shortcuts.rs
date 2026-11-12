//! Shared shortcut rule structure. Runtime generation and configuration UI are separate tasks.
//! This module deliberately does not read clocks or platform keyboard state.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_RULES: usize = 1024;
pub const MAX_RULES_PER_CODE: usize = 32;
pub const MAX_CODE_LEN: usize = 32;
pub const MAX_POSITION: u32 = 10_000;
pub const MAX_FORMAT_SCALARS: usize = 256;
pub const MAX_OUTPUT_SCALARS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutKind {
    DateTime,
}

/// Position is global and one-based; it is never a page-local shortcut key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortcutRule {
    pub code: String,
    pub position: u32,
    pub kind: ShortcutKind,
    pub format: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("快捷规则 {rule:?} 的 {field}：{message}")]
pub struct RuleError {
    /// Zero-based source row, for both frontends to focus the original draft.
    /// None denotes a whole-list error.
    pub rule: Option<usize>,
    pub field: &'static str,
    pub message: String,
}

fn error(rule: Option<usize>, field: &'static str, message: &str) -> RuleError {
    RuleError {
        rule,
        field,
        message: message.into(),
    }
}

/// Return canonical copies only after every row passes structural validation.
/// Disabled rules participate in limits and uniqueness. Drafts remain untouched on failure.
/// Compiles the fixed format grammar, but does not obtain time or render output.
pub fn validate_structure(rules: &[ShortcutRule]) -> Result<Vec<ShortcutRule>, RuleError> {
    if rules.len() > MAX_RULES {
        return Err(error(None, "rules", "最多 1024 条，包含停用规则"));
    }
    let mut identities = BTreeSet::new();
    let mut counts = BTreeMap::new();
    let mut normalized = Vec::with_capacity(rules.len());
    for (index, rule) in rules.iter().enumerate() {
        if rule.code.is_empty()
            || rule.code.len() > MAX_CODE_LEN
            || !rule.code.bytes().all(|c| c.is_ascii_alphabetic())
        {
            return Err(error(
                Some(index),
                "code",
                "编码须为 1～32 个 ASCII 英文字母",
            ));
        }
        if !(1..=MAX_POSITION).contains(&rule.position) {
            return Err(error(Some(index), "position", "全局位置须为 1～10000"));
        }
        if rule.format.chars().count() > MAX_FORMAT_SCALARS {
            return Err(error(Some(index), "format", "格式最多 256 个 Unicode 字符"));
        }
        if let Err(offset) = crate::shortcut_format::compile(&rule.format) {
            return Err(RuleError {
                rule: Some(index),
                field: "format",
                message: format!("第 {} 个字符存在未知格式标记或未闭合引号", offset + 1),
            });
        }
        let code = rule.code.to_ascii_lowercase();
        if !identities.insert((code.clone(), rule.position)) {
            return Err(error(
                Some(index),
                "position",
                "同一规范编码及全局位置已存在，包含停用规则",
            ));
        }
        let count = counts.entry(code.clone()).or_insert(0);
        *count += 1;
        if *count > MAX_RULES_PER_CODE {
            return Err(error(
                Some(index),
                "code",
                "同一编码最多 32 条，包含停用规则",
            ));
        }
        normalized.push(ShortcutRule {
            code,
            ..rule.clone()
        });
    }
    Ok(normalized)
}

/// Callers supply active table bindings; the core does not inspect OS key state.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShortcutEnvironment {
    pub wildcard: Option<char>,
    pub auxiliary_entry: Option<char>,
}

impl From<&crate::dictionary::TableConfig> for ShortcutEnvironment {
    fn from(table: &crate::dictionary::TableConfig) -> Self {
        Self {
            wildcard: table.wildcard_key,
            auxiliary_entry: table.temporary.trigger(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAvailability {
    /// Compatibility is independent of the user's enable intent.
    pub compatible: bool,
    pub effective: bool,
    pub reason: Option<String>,
}

/// An environment conflict pauses a rule without making it structurally invalid.
/// Recompute after each active table/binding change; never persist this status as `enabled`.
pub fn availability(rule: &ShortcutRule, environment: ShortcutEnvironment) -> RuleAvailability {
    let code = rule.code.to_ascii_lowercase();
    let conflict = [
        (environment.wildcard, "万能键"),
        (environment.auxiliary_entry, "辅助入口键"),
    ]
    .into_iter()
    .find_map(|(key, label)| {
        key.filter(|c| code.contains(c.to_ascii_lowercase()))
            .map(|key| format!("编码包含当前{label} {key}，此规则暂停"))
    });
    RuleAvailability {
        compatible: conflict.is_none(),
        effective: rule.enabled && conflict.is_none(),
        reason: conflict,
    }
}
