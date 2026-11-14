//! Bounded, version-bound input positions. A group is one position, never literal text.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_POSITIONS: usize = 32;
pub const MAX_GROUP: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappingIdentity {
    pub id: String,
    pub version: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub charset: String,
    pub groups: BTreeMap<String, String>,
}
impl Definition {
    pub fn validate(&self) -> Result<(), String> {
        if self.charset.is_empty()
            || self.charset.chars().count() > 128
            || self.charset.chars().any(char::is_control)
            || self.charset.chars().collect::<BTreeSet<_>>().len() != self.charset.chars().count()
            || self.groups.is_empty()
            || self.groups.len() > 128
        {
            return Err("映射字符集或组数量无效".into());
        }
        for (id, chars) in &self.groups {
            let count = chars.chars().count();
            if !crate::skin::valid_id(id)
                || count == 0
                || count > MAX_GROUP
                || chars.chars().collect::<BTreeSet<_>>().len() != count
                || !chars.chars().all(|c| self.charset.contains(c))
            {
                return Err(format!("非法字符组：{id}"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub characters: String,
    pub selected: Option<char>,
}
impl Position {
    pub fn exact(c: char) -> Self {
        Self {
            characters: c.to_string(),
            selected: None,
        }
    }
    pub fn effective(&self) -> String {
        self.selected
            .map(|c| c.to_string())
            .unwrap_or_else(|| self.characters.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    pub mapping: MappingIdentity,
    pub positions: Vec<Position>,
    /// Resolution history is separate from raw input and can be undone without losing a position.
    pub resolutions: Vec<(usize, Option<char>)>,
}
impl Sequence {
    pub fn resolved(&self) -> Option<String> {
        self.positions
            .iter()
            .map(|p| {
                p.selected.or_else(|| {
                    let mut chars = p.characters.chars();
                    let first = chars.next()?;
                    chars.next().is_none().then_some(first)
                })
            })
            .collect()
    }
    pub fn hint(&self) -> String {
        self.positions
            .iter()
            .map(|p| {
                let effective = p.effective();
                if effective.chars().count() == 1 {
                    effective
                } else {
                    format!("[{effective}]")
                }
            })
            .collect()
    }
    pub fn resolve(&mut self, index: usize, character: char) -> Result<(), String> {
        let position = self.positions.get_mut(index).ok_or("输入位置已失效")?;
        if !position.characters.contains(character) {
            return Err("指定字母不属于原始字符组".into());
        }
        if position.selected != Some(character) {
            self.resolutions.push((index, position.selected));
            position.selected = Some(character);
        }
        Ok(())
    }
    pub fn undo(&mut self) -> bool {
        if let Some((index, selected)) = self.resolutions.pop() {
            if let Some(position) = self.positions.get_mut(index) {
                position.selected = selected;
            }
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BoundMapping {
    pub identity: MappingIdentity,
    pub definition: Definition,
}
