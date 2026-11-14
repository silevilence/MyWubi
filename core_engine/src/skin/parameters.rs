//! Shared scalar resolution for native renderers and authoring previews.
use super::{Error, Extensions, Package};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const COLORS: &[&str] = &[
    "background",
    "text",
    "label",
    "hint",
    "code",
    "selected_background",
    "selected_text",
    "selected_hint",
    "disabled",
    "status",
    "border",
    "key_background",
    "key_text",
    "function_background",
    "function_text",
    "pressed_background",
    "pressed_text",
    "bubble_background",
    "bubble_text",
];

/// A complete, validated appearance. Numeric units are pt on Windows text, sp on
/// Android text, and DIP/dp for spacing. Layout/mapping are never merged here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameters(pub Extensions);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appearance {
    pub default_variant: String,
    #[serde(default)]
    pub defaults: Extensions,
    pub variants: BTreeMap<String, Extensions>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub images: super::images::Images,
    pub parameters: Parameters,
    pub variant: String,
    pub warnings: Vec<String>,
}

/// Explicit conversion of legacy ARGB; new eight-digit color strings are RGBA.
pub fn argb_to_color(value: u32) -> String {
    format!("#{:06X}{:02X}", value & 0xffffff, value >> 24)
}

pub fn parse_color(value: &str) -> Result<u32, Error> {
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| Error::Invalid(format!("颜色需要 #：{value}")))?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Invalid(format!("非法颜色：{value}")));
    }
    let raw =
        u32::from_str_radix(hex, 16).map_err(|_| Error::Invalid(format!("非法颜色：{value}")))?;
    match hex.len() {
        6 => Ok(0xff000000 | raw),
        8 => Ok((raw >> 8) | ((raw & 255) << 24)),
        _ => Err(Error::Invalid(format!("颜色长度非法：{value}"))),
    }
}

pub fn validate_patch(patch: &Extensions) -> Result<(), Error> {
    for (key, value) in patch {
        let valid = if COLORS.contains(&key.as_str()) {
            value.as_str().is_some_and(|s| parse_color(s).is_ok())
        } else {
            match key.as_str() {
                "font_family" => value
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty() && s.len() <= 256),
                "layout" => value
                    .as_str()
                    .is_some_and(|s| matches!(s, "auto" | "horizontal" | "vertical")),
                "density" => value
                    .as_str()
                    .is_some_and(|s| matches!(s, "compact" | "comfortable")),
                "font_size" | "hint_size" | "key_text_size" | "bubble_text_size" => {
                    number(value).is_some_and(|v| (8.0..=96.0).contains(&v))
                }
                "padding_x" | "padding_y" | "corner_radius" | "selected_radius" | "key_radius"
                | "key_gap" | "shadow" => number(value).is_some_and(|v| (0.0..=64.0).contains(&v)),
                "border_width" => number(value).is_some_and(|v| (0.0..=8.0).contains(&v)),
                "max_width" => number(value).is_some_and(|v| (100.0..=2000.0).contains(&v)),
                "keyboard_height" => number(value).is_some_and(|v| (160.0..=600.0).contains(&v)),
                _ => false,
            }
        };
        if !valid {
            return Err(Error::Invalid(format!("非法外观字段或值：{key}")));
        }
    }
    Ok(())
}

fn number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|n| n as f64))
}

impl Parameters {
    pub fn color(&self, key: &str) -> u32 {
        self.0
            .get(key)
            .and_then(toml::Value::as_str)
            .and_then(|s| parse_color(s).ok())
            .unwrap_or(0xff000000)
    }
    pub fn number(&self, key: &str) -> f32 {
        self.0.get(key).and_then(number).unwrap_or(0.0) as f32
    }
    pub fn text(&self, key: &str) -> &str {
        self.0.get(key).and_then(toml::Value::as_str).unwrap_or("")
    }
    pub fn overlay(&mut self, patch: &Extensions) -> Result<(), Error> {
        validate_patch(patch)?;
        self.0.extend(patch.clone());
        Ok(())
    }
}

fn overlay_layer(
    effective: &mut Parameters,
    layer: &Extensions,
    variant: &str,
) -> Result<(), Error> {
    let mut common = layer.clone();
    for mode in ["light", "dark"] {
        if let Some(value) = common.remove(mode) {
            let table = value
                .as_table()
                .ok_or_else(|| Error::Invalid(format!("{mode} 色板必须为表")))?;
            let patch = table.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            validate_patch(&patch)?;
        }
    }
    effective.overlay(&common)?;
    if let Some(table) = layer.get(variant).and_then(toml::Value::as_table) {
        effective.overlay(&table.iter().map(|(k, v)| (k.clone(), v.clone())).collect())?;
    }
    Ok(())
}

/// Editable copies of built-ins use exactly the same contract as imported packages.
pub fn builtin_package(name: &str) -> Result<Package, Error> {
    let mut files = BTreeMap::new();
    let metadata = super::Metadata {
        format: 1,
        id: format!("mywubi.{name}"),
        version: "1".into(),
        name: name.into(),
        author: "MyWubi".into(),
        license: "MIT".into(),
        capabilities: super::Capabilities {
            required: vec!["container.zip/1".into()],
            ..Default::default()
        },
        platforms: [
            ("windows".into(), "windows/metadata.toml".into()),
            ("android".into(), "android/metadata.toml".into()),
        ]
        .into(),
        resources: BTreeMap::new(),
        defaults: Extensions::new(),
        extensions: Extensions::new(),
    };
    files.insert(
        "metadata.toml".into(),
        toml::to_string(&metadata)?.into_bytes(),
    );
    for platform in ["windows", "android"] {
        let appearance = Appearance {
            default_variant: "light".into(),
            defaults: Extensions::new(),
            extensions: Extensions::new(),
            variants: [
                ("light".into(), builtin(name, false)?.0),
                ("dark".into(), builtin(name, true)?.0),
            ]
            .into(),
        };
        let descriptor = super::Platform {
            platform: platform.into(),
            capabilities: super::Capabilities {
                required: vec![format!("{platform}.base/1")],
                ..Default::default()
            },
            defaults: Extensions::new(),
            resources: BTreeMap::new(),
            extensions: Extensions::new(),
            components: vec![super::Component {
                id: name.into(),
                version: "1".into(),
                kind: "appearance".into(),
                path: "appearance.toml".into(),
                dependencies: BTreeMap::new(),
                extensions: Extensions::new(),
                capabilities: super::Capabilities {
                    required: vec!["appearance.parameters/1".into()],
                    ..Default::default()
                },
            }],
        };
        files.insert(
            format!("{platform}/metadata.toml"),
            toml::to_string(&descriptor)?.into_bytes(),
        );
        files.insert(
            format!("{platform}/appearance.toml"),
            toml::to_string(&appearance)?.into_bytes(),
        );
    }
    Package::from_files(files)
}

pub fn builtin(name: &str, dark: bool) -> Result<Parameters, Error> {
    let colors = match (name, dark) {
        ("jade", false) => ["#F4F7F5", "#192C27", "#52635B", "#245D51", "#FFFFFF"],
        ("jade", true) => ["#191F1D", "#E1E8E4", "#AFBEB6", "#9CD3C1", "#00382C"],
        ("plain", false) => ["#FAFAFA", "#202020", "#595959", "#333333", "#FFFFFF"],
        ("plain", true) => ["#202020", "#FAFAFA", "#BDBDBD", "#E0E0E0", "#202020"],
        ("contrast", false) => ["#FFFFFF", "#000000", "#333333", "#000000", "#FFFFFF"],
        ("contrast", true) => ["#000000", "#FFFFFF", "#EEEEEE", "#FFFFFF", "#000000"],
        _ => return Err(Error::Invalid(format!("未知内置皮肤：{name}"))),
    };
    let [background, text, hint, selected, selected_text] = colors;
    let mut values: Extensions = [
        ("background", background),
        ("text", text),
        ("label", hint),
        ("hint", hint),
        ("code", selected),
        ("selected_background", selected),
        ("selected_text", selected_text),
        ("selected_hint", selected_text),
        ("disabled", hint),
        ("status", hint),
        ("border", hint),
        ("key_background", background),
        ("key_text", text),
        ("function_background", selected),
        ("function_text", selected_text),
        ("pressed_background", selected),
        ("pressed_text", selected_text),
        ("bubble_background", selected),
        ("bubble_text", selected_text),
        ("font_family", "Microsoft YaHei UI"),
        ("layout", "auto"),
        ("density", "compact"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), toml::Value::String(v.into())))
    .collect();
    for (k, v) in [
        ("font_size", 14.0),
        ("hint_size", 11.0),
        ("key_text_size", 20.0),
        ("bubble_text_size", 28.0),
        ("padding_x", 8.0),
        ("padding_y", 6.0),
        ("corner_radius", 8.0),
        ("selected_radius", 4.0),
        ("border_width", 1.0),
        ("shadow", if name == "contrast" { 0.0 } else { 4.0 }),
        ("max_width", 900.0),
        ("key_radius", 6.0),
        ("key_gap", 4.0),
        ("keyboard_height", 240.0),
    ] {
        values.insert(k.into(), toml::Value::Float(v));
    }
    Ok(Parameters(values))
}

/// No selection means old colors and point-size keep their original interpretation.
pub fn legacy(appearance: &crate::config::Appearance) -> Result<Parameters, Error> {
    let mut p = builtin("plain", false)?;
    let [_, r, g, b] = appearance.background_color.to_be_bytes();
    let foreground = if u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 < 128000 {
        0xfffafafa
    } else {
        0xff202020
    };
    for key in [
        "text",
        "label",
        "hint",
        "selected_text",
        "selected_hint",
        "status",
    ] {
        p.0.insert(key.into(), argb_to_color(foreground).into());
    }
    for (key, value) in [
        ("background", appearance.background_color),
        ("code", appearance.primary_color),
        ("selected_background", appearance.highlight_color),
    ] {
        p.0.insert(key.into(), argb_to_color(value).into());
    }
    p.0.insert(
        "font_size".into(),
        toml::Value::Integer(appearance.font_size.into()),
    );
    for key in ["corner_radius", "border_width", "shadow"] {
        p.0.insert(key.into(), 0.into());
    }
    Ok(p)
}

/// Resolve only a supported appearance; validate every declared variant before activation.
pub fn resolve(
    package: &Package,
    platform: &str,
    component_id: &str,
    dark: bool,
    overrides: &Extensions,
    high_contrast: bool,
    supported: &BTreeSet<String>,
) -> Result<Resolved, Error> {
    let (base, availability) =
        package.compatibility(platform, supported, &["appearance".into()].into())?;
    let p = &package.platforms[platform];
    let component = p
        .components
        .iter()
        .find(|c| c.id == component_id && c.kind == "appearance")
        .ok_or_else(|| Error::Invalid("未找到外观组件".into()))?;
    if !base.enabled || !availability[component_id].enabled {
        return Err(Error::Invalid(
            availability[component_id].reasons.join("；"),
        ));
    }
    if !component
        .capabilities
        .required
        .iter()
        .any(|c| c == "appearance.parameters/1")
    {
        return Err(Error::Invalid(
            "外观组件未声明 appearance.parameters/1".into(),
        ));
    }
    let appearance: Appearance =
        super::parse(&package.files, &format!("{platform}/{}", component.path))?;
    if !appearance
        .variants
        .contains_key(&appearance.default_variant)
        || appearance
            .variants
            .keys()
            .any(|k| !matches!(k.as_str(), "light" | "dark"))
    {
        return Err(Error::Invalid("外观默认变体或变体名称无效".into()));
    }
    validate_patch(&appearance.defaults)?;
    for patch in appearance.variants.values() {
        validate_patch(patch)?;
    }
    let requested = if dark { "dark" } else { "light" };
    let variant = if appearance.variants.contains_key(requested) {
        requested
    } else {
        &appearance.default_variant
    };
    let mut warnings = Vec::new();
    if requested != variant {
        warnings.push(format!("缺少 {requested} 变体，使用 {variant}"));
    }
    let mut effective = builtin("jade", variant == "dark")?;
    for layer in [&package.metadata.defaults, &p.defaults] {
        overlay_layer(&mut effective, layer, variant)?;
    }
    for layer in [
        &appearance.defaults,
        &appearance.variants[variant],
        overrides,
    ] {
        effective.overlay(layer)?;
    }
    for (cap, fallback) in base
        .fallbacks
        .iter()
        .chain(availability[component_id].fallbacks.iter())
    {
        if fallback != "none" && fallback != "plain" {
            return Err(Error::Invalid(format!("无法解释回退：{cap}={fallback}")));
        }
        warnings.push(format!("{cap} 不可用，回退为 {fallback}"));
    }
    if high_contrast {
        let contrast = builtin("contrast", dark)?;
        for key in COLORS.iter().copied().chain(["shadow", "corner_radius"]) {
            if let Some(value) = contrast.0.get(key) {
                effective.0.insert(key.into(), value.clone());
            }
        }
        effective.0.insert("border_width".into(), 1.into());
    }
    let images = super::images::resolve(
        package,
        platform,
        component,
        &appearance,
        variant,
        supported,
    )?;
    Ok(Resolved {
        images: if high_contrast {
            Default::default()
        } else {
            images
        },
        parameters: effective,
        variant: variant.into(),
        warnings,
    })
}
