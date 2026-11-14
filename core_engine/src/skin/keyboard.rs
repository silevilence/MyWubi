//! Versioned keyboard declarations. Parsing does not activate a keyboard.
use super::{Component, Error, Extensions, Package};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const LAYOUT_CAPABILITY: &str = "keyboard.layout/1";
pub const GESTURE_CAPABILITY: &str = "keyboard.directions8/1";
pub const MERGE_CAPABILITY: &str = "keyboard.direction-groups/1";
pub const GROUP_CAPABILITY: &str = "input.groups/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Main,
    Pinyin,
    English,
    Number,
    Phone,
    Symbols,
}

/// Only these semantic operations may be dispatched by an untrusted package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum KeyAction {
    Character(char),
    Group(String),
    Backspace,
    Space,
    Enter,
    Cancel,
    PageNext,
    PagePrevious,
    Letters,
    Numbers,
    Symbols,
    Shift,
    Pinyin,
    English,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    pub format: u32,
    pub charset: String,
    pub modes: Vec<Mode>,
    pub groups: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    pub id: String,
    pub label: String,
    /// Coordinates in a layout's logical canvas, independent of its decoration.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub role: String,
    pub action: KeyAction,
    pub long_press: Option<KeyAction>,
    #[serde(default)]
    pub directions: BTreeMap<String, KeyAction>,
    #[serde(default)]
    pub disabled_directions: Vec<String>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub format: u32,
    pub modes: Vec<Mode>,
    pub charset: String,
    pub width: f32,
    pub height: f32,
    pub mapping: Option<String>,
    pub recommended_appearance: Option<String>,
    pub fallback: String,
    /// Semantic roles may fall back to built-in painting; fixed artwork may not.
    #[serde(default)]
    pub required_styles: Vec<String>,
    #[serde(default)]
    pub optional_styles: BTreeMap<String, String>,
    #[serde(default)]
    pub directions: BTreeMap<String, KeyAction>,
    pub keys: Vec<Key>,
    /// An optional second canvas; it is checked by the same geometry validator.
    pub landscape: Option<Box<Layout>>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Mapping {
    pub fn validate(&self) -> Result<(), Error> {
        validate_header(self.format, &self.charset, &self.modes)?;
        if self.groups.is_empty() || self.groups.len() > 128 {
            return invalid("映射分组数量必须为 1..128");
        }
        for (id, chars) in &self.groups {
            let unique: BTreeSet<_> = chars.chars().collect();
            if !super::valid_id(id)
                || unique.is_empty()
                || unique.len() > 32
                || unique.len() != chars.chars().count()
                || !chars.chars().all(|c| self.charset.contains(c))
            {
                return invalid(&format!("分组 {id} 含空集、重复或非法字符"));
            }
        }
        Ok(())
    }
}

fn validate_header(format: u32, charset: &str, modes: &[Mode]) -> Result<(), Error> {
    if format != 1
        || charset.is_empty()
        || charset.chars().count() > 128
        || charset.chars().any(char::is_control)
        || charset.chars().collect::<BTreeSet<_>>().len() != charset.chars().count()
        || modes.is_empty()
    {
        return invalid("键盘格式、字符集或模式无效");
    }
    Ok(())
}

fn invalid<T>(message: &str) -> Result<T, Error> {
    Err(Error::Invalid(message.into()))
}

/// Validate the component dependency, not merely the presence of a similarly named file.
pub fn mapping(package: &Package, component: &Component) -> Result<Mapping, Error> {
    if component.kind != "mapping" || !component.path.starts_with("mappings/") {
        return invalid("映射须位于 android/mappings/");
    }
    let mapping: Mapping = super::parse(&package.files, &format!("android/{}", component.path))?;
    mapping.validate()?;
    Ok(mapping)
}

pub fn layout(
    package: &Package,
    component: &Component,
) -> Result<(Layout, Option<Mapping>), Error> {
    if component.kind != "layout" || !component.path.starts_with("layouts/") {
        return invalid("布局须位于 android/layouts/");
    }
    if !component
        .capabilities
        .required
        .iter()
        .any(|c| c == LAYOUT_CAPABILITY)
    {
        return invalid("布局缺少 keyboard.layout/1 能力声明");
    }
    let layout: Layout = super::parse(&package.files, &format!("android/{}", component.path))?;
    validate_header(layout.format, &layout.charset, &layout.modes)?;
    for key in &layout.keys {
        super::directions::resolve(
            &layout.directions,
            &key.directions,
            &key.disabled_directions,
        )
        .map_err(|e| Error::Invalid(format!("按键 {}：{e}", key.id)))?;
    }
    if !super::valid_id(&layout.fallback) || layout.optional_styles.values().any(|v| v.is_empty()) {
        return invalid("布局回退或可选样式回退无效");
    }
    if (!layout.directions.is_empty() || layout.keys.iter().any(|k| !k.directions.is_empty()))
        && !component
            .capabilities
            .required
            .iter()
            .any(|c| c == GESTURE_CAPABILITY)
    {
        return invalid("方向动作缺少 keyboard.directions8/1 能力声明");
    }
    if layout
        .directions
        .keys()
        .chain(layout.keys.iter().flat_map(|k| k.directions.keys()))
        .any(|d| d.contains('+'))
        && !component
            .capabilities
            .required
            .iter()
            .any(|c| c == MERGE_CAPABILITY)
    {
        return invalid("方向合并缺少 keyboard.direction-groups/1 能力声明");
    }
    let mapping = layout
        .mapping
        .as_ref()
        .map(|id| {
            let version = component
                .dependencies
                .get(id)
                .ok_or_else(|| Error::Invalid(format!("布局未声明映射依赖：{id}")))?;
            let declaration = package
                .platforms
                .get("android")
                .and_then(|p| {
                    p.components
                        .iter()
                        .find(|c| &c.id == id && &c.version == version)
                })
                .ok_or_else(|| Error::Invalid(format!("缺少映射：{id}@{version}")))?;
            mapping(package, declaration)
        })
        .transpose()?;
    let actions = layout
        .keys
        .iter()
        .flat_map(|k| {
            std::iter::once(&k.action)
                .chain(k.long_press.iter())
                .chain(k.directions.values())
        })
        .chain(layout.directions.values());
    for action in actions {
        if let KeyAction::Group(id) = action {
            if !component
                .capabilities
                .required
                .iter()
                .any(|c| c == GROUP_CAPABILITY)
                || !mapping.as_ref().is_some_and(|m| {
                    m.groups.contains_key(id)
                        && layout.modes.iter().all(|mode| m.modes.contains(mode))
                        && layout.charset.chars().all(|c| m.charset.contains(c))
                })
            {
                return invalid(&format!("分组 {id} 缺少兼容映射或 input.groups/1 能力"));
            }
        }
    }
    Ok((layout, mapping))
}

/// Content validation augments ordinary dependency compatibility without disabling paint.
pub fn compatibility(
    package: &Package,
    supported: &BTreeSet<String>,
) -> Result<BTreeMap<String, super::Availability>, Error> {
    let (_, mut states) = package.compatibility(
        "android",
        supported,
        &["appearance".into(), "layout".into(), "mapping".into()].into(),
    )?;
    let platform = &package.platforms["android"];
    for component in &platform.components {
        let result = match component.kind.as_str() {
            "layout" if states[&component.id].enabled => layout(package, component).map(|_| ()),
            "mapping" if states[&component.id].enabled => mapping(package, component).map(|_| ()),
            _ => Ok(()),
        };
        if let Err(error) = result {
            if let Some(state) = states.get_mut(&component.id) {
                state.enabled = false;
                state.reasons.push(error.to_string());
            }
        }
    }
    loop {
        let disabled: Vec<_> = platform
            .components
            .iter()
            .filter(|c| {
                states[&c.id].enabled
                    && c.dependencies
                        .keys()
                        .any(|id| states.get(id).is_none_or(|s| !s.enabled))
            })
            .map(|c| c.id.clone())
            .collect();
        if disabled.is_empty() {
            break;
        }
        for id in disabled {
            if let Some(state) = states.get_mut(&id) {
                state.enabled = false;
                state.reasons.push("键盘依赖内容无效".into());
            }
        }
    }
    Ok(states)
}
