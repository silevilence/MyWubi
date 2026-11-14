//! Portable skin metadata and compatibility. Loading never changes input actions.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod archive;

/// Unknown TOML fields are retained when an editor changes a known field.
pub type Extensions = BTreeMap<String, toml::Value>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("皮肤包无效：{0}")]
    Invalid(String),
    #[error("皮肤 metadata 格式错误：{0}")]
    Toml(#[from] toml::de::Error),
    #[error("皮肤文件读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("皮肤 ZIP 无效：{0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("皮肤 metadata 无法保存：{0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("皮肤 {0}@{1} 已存在且内容不同，请另存新版本")]
    Conflict(String, String),
}

/// Capability IDs include their major version, e.g. `appearance.parameters/1`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub required: Vec<String>,
    /// Unsupported optional capabilities must name an explicit fallback.
    #[serde(default)]
    pub optional: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub format: u32,
    pub id: String,
    pub version: String,
    pub name: String,
    pub author: String,
    pub license: String,
    #[serde(default)]
    pub capabilities: Capabilities,
    pub platforms: BTreeMap<String, String>,
    #[serde(default)]
    pub resources: BTreeMap<String, String>,
    #[serde(default)]
    pub defaults: Extensions,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub platform: String,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub defaults: Extensions,
    #[serde(default)]
    pub resources: BTreeMap<String, String>,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Exact component versions within this platform; never silently use latest.
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Availability {
    pub enabled: bool,
    pub reasons: Vec<String>,
    pub fallbacks: BTreeMap<String, String>,
}

impl Capabilities {
    fn check(&self, supported: &BTreeSet<String>) -> Availability {
        let reasons: Vec<_> = self
            .required
            .iter()
            .filter(|c| !supported.contains(*c))
            .map(|c| format!("缺少必需能力：{c}"))
            .collect();
        Availability {
            enabled: reasons.is_empty(),
            reasons,
            fallbacks: self
                .optional
                .iter()
                .filter(|(c, _)| !supported.contains(*c))
                .map(|(c, fallback)| (c.clone(), fallback.clone()))
                .collect(),
        }
    }
    fn validate(&self) -> Result<(), Error> {
        let mut seen = BTreeSet::new();
        for cap in self.required.iter().chain(self.optional.keys()) {
            if cap.is_empty() || !seen.insert(cap) || !cap.contains('/') {
                return Err(Error::Invalid(format!("能力声明重复或无版本：{cap}")));
            }
        }
        if self.optional.values().any(|v| v.trim().is_empty()) {
            return Err(Error::Invalid("可选能力缺少回退声明".into()));
        }
        Ok(())
    }
}

/// Strict portable path syntax prevents Windows aliases as well as ZIP traversal.
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 240
        && path.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
                && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        })
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && id != "."
        && id != ".."
}

/// Parsed documents alongside original bytes. Unknown components/resources stay intact.
#[derive(Debug, Clone)]
pub struct Package {
    pub metadata: Metadata,
    pub platforms: BTreeMap<String, Platform>,
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Package {
    pub fn from_files(files: BTreeMap<String, Vec<u8>>) -> Result<Self, Error> {
        let mut names = BTreeSet::new();
        for name in files.keys() {
            if !valid_path(name) || !names.insert(name.to_lowercase()) {
                return Err(Error::Invalid(format!("非法或大小写冲突路径：{name}")));
            }
            let mut parent = name.as_str();
            while let Some((prefix, _)) = parent.rsplit_once('/') {
                if files.keys().any(|p| p.eq_ignore_ascii_case(prefix)) {
                    return Err(Error::Invalid(format!("文件与目录冲突：{name}")));
                }
                parent = prefix;
            }
        }
        let metadata: Metadata = parse(&files, "metadata.toml")?;
        if metadata.format != 1
            || !valid_id(&metadata.id)
            || !valid_id(&metadata.version)
            || metadata.name.trim().is_empty()
            || metadata.platforms.is_empty()
        {
            return Err(Error::Invalid("根格式、身份或平台入口无效".into()));
        }
        metadata.capabilities.validate()?;
        validate_resources(&files, &metadata.resources, "shared/", "")?;
        let mut platforms = BTreeMap::new();
        for (name, path) in &metadata.platforms {
            if !matches!(name.as_str(), "windows" | "android")
                || path != &format!("{name}/metadata.toml")
            {
                return Err(Error::Invalid(format!("非法平台入口：{name}")));
            }
            let platform: Platform = parse(&files, path)?;
            if platform.components.len() > 128 {
                return Err(Error::Invalid("平台组件数量超过 128".into()));
            }
            if &platform.platform != name {
                return Err(Error::Invalid("总分平台声明不一致".into()));
            }
            platform.capabilities.validate()?;
            validate_resources(&files, &platform.resources, "assets/", &format!("{name}/"))?;
            let mut ids = BTreeSet::new();
            let mut paths = BTreeSet::new();
            for component in &platform.components {
                if !valid_id(&component.id)
                    || !valid_id(&component.version)
                    || !ids.insert(&component.id)
                    || !valid_path(&component.path)
                    || !paths.insert(component.path.to_lowercase())
                    || component.path == "metadata.toml"
                    || component.kind.is_empty()
                {
                    return Err(Error::Invalid("组件身份或路径无效/重复".into()));
                }
                component.capabilities.validate()?;
                let _: toml::Value = parse(&files, &format!("{name}/{}", component.path))?;
                if component
                    .dependencies
                    .iter()
                    .any(|(id, v)| !valid_id(id) || !valid_id(v))
                {
                    return Err(Error::Invalid("组件依赖身份无效".into()));
                }
            }
            platforms.insert(name.clone(), platform);
        }
        Ok(Self {
            metadata,
            platforms,
            files,
        })
    }

    /// Container failure rejects the package, platform failure does not affect other platforms.
    pub fn compatibility(
        &self,
        platform: &str,
        supported: &BTreeSet<String>,
        kinds: &BTreeSet<String>,
    ) -> Result<(Availability, BTreeMap<String, Availability>), Error> {
        let root = self.metadata.capabilities.check(supported);
        if !root.enabled {
            return Err(Error::Invalid(root.reasons.join("；")));
        }
        let p = self
            .platforms
            .get(platform)
            .ok_or_else(|| Error::Invalid(format!("不支持平台：{platform}")))?;
        let base = p.capabilities.check(supported);
        let mut states = BTreeMap::new();
        for c in &p.components {
            let mut state = c.capabilities.check(supported);
            if !base.enabled {
                state.reasons.extend(base.reasons.iter().cloned());
            }
            if !kinds.contains(&c.kind) {
                state.reasons.push(format!("未知组件种类：{}", c.kind));
            }
            for (id, version) in &c.dependencies {
                if !p
                    .components
                    .iter()
                    .any(|d| &d.id == id && &d.version == version)
                {
                    state.reasons.push(format!("缺少依赖版本：{id}@{version}"));
                }
            }
            if dependency_cycle(&c.id, &p.components, &mut BTreeSet::new()) {
                state.reasons.push("组件依赖循环".into());
            }
            state.enabled = state.reasons.is_empty();
            states.insert(c.id.clone(), state);
        }
        loop {
            let disabled: Vec<_> = p
                .components
                .iter()
                .filter(|c| states[&c.id].enabled)
                .filter(|c| {
                    c.dependencies
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
                    state.reasons.push("依赖组件不可用".into());
                }
            }
        }
        let mut base = base;
        base.fallbacks.extend(root.fallbacks);
        Ok((base, states))
    }
}

fn dependency_cycle<'a>(
    id: &'a str,
    components: &'a [Component],
    visiting: &mut BTreeSet<&'a str>,
) -> bool {
    if !visiting.insert(id) {
        return true;
    }
    let cycle = components.iter().find(|c| c.id == id).is_some_and(|c| {
        c.dependencies
            .keys()
            .any(|dep| dependency_cycle(dep, components, visiting))
    });
    visiting.remove(id);
    cycle
}

fn parse<T: serde::de::DeserializeOwned>(
    files: &BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<T, Error> {
    let bytes = files
        .get(path)
        .ok_or_else(|| Error::Invalid(format!("缺少文件：{path}")))?;
    let source = std::str::from_utf8(bytes)
        .map_err(|_| Error::Invalid(format!("非 UTF-8 metadata：{path}")))?;
    Ok(toml::from_str(source)?)
}

fn validate_resources(
    files: &BTreeMap<String, Vec<u8>>,
    resources: &BTreeMap<String, String>,
    prefix: &str,
    base: &str,
) -> Result<(), Error> {
    for (id, path) in resources {
        if !valid_id(id)
            || !valid_path(path)
            || !path.starts_with(prefix)
            || !files.contains_key(&format!("{base}{path}"))
        {
            return Err(Error::Invalid(format!("资源引用无效：{id} → {path}")));
        }
    }
    Ok(())
}
