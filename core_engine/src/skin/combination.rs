//! Resolve and publish complete keyboard combinations. A choice is never an active snapshot.
use super::{
    keyboard::{self, KeyAction, Layout, Mapping, Mode},
    preferences::{Preference, Selection},
    runtime, Error, Package,
};
use crate::group_input::{Definition, MappingIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub layout: Selection,
    pub mapping: Option<Selection>,
    pub appearance: Preference,
}

pub struct Snapshot {
    pub choice: Choice,
    pub layout: Layout,
    pub mapping: Option<Mapping>,
    pub mapping_identity: Option<MappingIdentity>,
    pub revision: u64,
    pub digest: String,
    pub resource_digests: BTreeMap<String, String>,
    pub role_fallbacks: BTreeMap<String, String>,
    pub light: runtime::Snapshot,
    pub dark: runtime::Snapshot,
    /// In-flight events and sessions pin the complete bytes, even after a newer publication.
    pub packages: Vec<Arc<Package>>,
}

pub fn supported() -> BTreeSet<String> {
    [
        "container.zip/1",
        "android.base/1",
        "appearance.parameters/1",
        super::images::CAPABILITY,
        keyboard::LAYOUT_CAPABILITY,
        keyboard::GROUP_CAPABILITY,
        keyboard::GESTURE_CAPABILITY,
        keyboard::MERGE_CAPABILITY,
    ]
    .map(str::to_owned)
    .into()
}

fn package(path: &Path, selection: &Selection) -> Result<Arc<Package>, Error> {
    Preference {
        selection: Some(selection.clone()),
        ..Default::default()
    }
    .validate()?;
    let package = Package::open(&runtime::store_path(path).join(format!(
        "{}@{}.mywubi-skin",
        selection.id, selection.version
    )))?;
    if package.metadata.id != selection.id || package.metadata.version != selection.version {
        return Err(Error::Invalid("组合包身份与固定版本不符".into()));
    }
    Ok(Arc::new(package))
}
fn component<'a>(
    package: &'a Package,
    selection: &Selection,
) -> Result<&'a super::Component, Error> {
    let states = keyboard::compatibility(package, &supported())?;
    let state = states
        .get(&selection.component)
        .ok_or_else(|| Error::Invalid("组合组件不存在".into()))?;
    if !state.enabled {
        return Err(Error::Invalid(state.reasons.join("；")));
    }
    package.platforms["android"]
        .components
        .iter()
        .find(|c| c.id == selection.component)
        .ok_or_else(|| Error::Invalid("组合组件不存在".into()))
}

pub fn resolve(path: &Path, choice: &Choice, mode: Mode, charset: &str) -> Result<Snapshot, Error> {
    let layout_package = package(path, &choice.layout)?;
    let declaration = component(&layout_package, &choice.layout)?;
    let (layout, mut mapping) = keyboard::layout(&layout_package, declaration)?;
    let mut mapping_selection = layout.mapping.as_ref().map(|id| Selection {
        id: choice.layout.id.clone(),
        version: choice.layout.version.clone(),
        component: id.clone(),
    });
    let mut mapping_version = layout
        .mapping
        .as_ref()
        .and_then(|id| declaration.dependencies.get(id))
        .cloned();
    let mut packages = vec![Arc::clone(&layout_package)];
    if let Some(selected) = &choice.mapping {
        let p = package(path, selected)?;
        let c = component(&p, selected)?;
        mapping = Some(keyboard::mapping(&p, c)?);
        mapping_version = Some(c.version.clone());
        mapping_selection = Some(selected.clone());
        packages.push(p);
    }
    if !layout.modes.contains(&mode) || !charset.chars().all(|c| layout.charset.contains(c)) {
        return Err(Error::Invalid("布局不支持当前输入模式或码表字符集".into()));
    }
    if let Some(mapping) = &mapping {
        mapping.validate()?;
        if !mapping.modes.contains(&mode)
            || !layout.charset.chars().all(|c| mapping.charset.contains(c))
        {
            return Err(Error::Invalid("映射与布局模式/字符集不兼容".into()));
        }
    }
    layout.validate_geometry(
        mapping.as_ref(),
        &[KeyAction::Space, KeyAction::Enter, KeyAction::Cancel],
    )?;
    let known: BTreeSet<_> = super::images::ROLES.iter().copied().collect();
    if layout
        .required_styles
        .iter()
        .any(|role| !known.contains(role.as_str()))
    {
        return Err(Error::Invalid("缺少布局必需样式角色".into()));
    }
    let mut role_fallbacks = BTreeMap::new();
    for key in layout
        .keys
        .iter()
        .chain(layout.landscape.iter().flat_map(|l| l.keys.iter()))
    {
        if !known.contains(key.role.as_str()) {
            let fallback = layout
                .optional_styles
                .get(&key.role)
                .filter(|role| known.contains(role.as_str()))
                .ok_or_else(|| {
                    Error::Invalid(format!("按键 {} 缺少样式角色 {} 的回退", key.id, key.role))
                })?;
            role_fallbacks.insert(key.role.clone(), fallback.clone());
        }
    }
    let mut config = crate::Config::default();
    config.skins.android = choice.appearance.clone();
    let light = runtime::load(path, &config, "android", false, false)?;
    let dark = runtime::load(path, &config, "android", true, false)?;
    if let Some(p) = &light.package {
        if let Some(selected) = &choice.appearance.selection {
            let c = component(p, selected)?;
            let appearance: super::parameters::Appearance =
                super::parse(&p.files, &format!("android/{}", c.path))?;
            if appearance.extensions.contains_key("fixed_layout") {
                return Err(Error::Invalid(
                    "固定键位图片不支持自定义布局，请选用语义角色外观".into(),
                ));
            }
        }
        packages.push(Arc::clone(p));
    }
    let mut resource_digests = BTreeMap::new();
    for p in &packages {
        for (name, bytes) in &p.files {
            resource_digests.insert(
                format!("{}@{}:{name}", p.metadata.id, p.metadata.version),
                format!("{:x}", Sha256::digest(bytes)),
            );
        }
    }
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(choice).map_err(|e| Error::Invalid(e.to_string()))?);
    for (name, digest) in &resource_digests {
        hash.update(name);
        hash.update([0]);
        hash.update(digest);
    }
    let digest_bytes = hash.finalize();
    let mut revision_bytes = [0; 8];
    revision_bytes.copy_from_slice(&digest_bytes[..8]);
    let revision = (u64::from_le_bytes(revision_bytes) & ((1 << 53) - 1)).max(1);
    let mapping_bytes = serde_json::to_vec(&(&mapping_selection, &mapping_version, &mapping))
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let mapping_digest = Sha256::digest(mapping_bytes);
    revision_bytes.copy_from_slice(&mapping_digest[..8]);
    let mapping_revision = (u64::from_le_bytes(revision_bytes) & ((1 << 53) - 1)).max(1);
    let mapping_identity = mapping_selection
        .zip(mapping_version)
        .map(|(selected, version)| MappingIdentity {
            id: selected.component,
            version,
            revision: mapping_revision,
        });
    Ok(Snapshot {
        choice: choice.clone(),
        layout,
        mapping,
        mapping_identity,
        revision,
        digest: format!("{digest_bytes:x}"),
        resource_digests,
        role_fallbacks,
        light,
        dark,
        packages,
    })
}

impl Snapshot {
    pub fn definition(&self) -> Option<Definition> {
        self.mapping.as_ref().map(|m| Definition {
            charset: m.charset.clone(),
            groups: m.groups.clone(),
        })
    }
}

/// Prepared snapshots are published as one Arc. A failed preparation cannot enter this API.
pub struct KeyboardRuntime {
    active: arc_swap::ArcSwap<Snapshot>,
}
impl KeyboardRuntime {
    pub fn new(snapshot: Snapshot) -> Self {
        Self {
            active: arc_swap::ArcSwap::from_pointee(snapshot),
        }
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.active.load_full()
    }
    pub fn refresh(
        &self,
        path: &Path,
        choice: &Choice,
        mode: Mode,
        charset: &str,
    ) -> Result<(), Error> {
        let next = resolve(path, choice, mode, charset)?;
        self.active.store(Arc::new(next));
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    active: Option<Choice>,
    digest: String,
    previous: Option<(Choice, String)>,
}
pub fn save(path: &Path, snapshot: &Snapshot) -> Result<(), Error> {
    let target = path.with_file_name("keyboard-selection.json");
    let old = std::fs::read(&target)
        .ok()
        .and_then(|b| serde_json::from_slice::<Receipt>(&b).ok());
    if old.as_ref().is_some_and(|r| r.digest == snapshot.digest) {
        return Ok(());
    }
    let mode = snapshot
        .layout
        .modes
        .first()
        .ok_or_else(|| Error::Invalid("布局缺少模式".into()))?;
    let previous = old.and_then(|r| {
        r.active
            .map(|choice| (choice, r.digest))
            .filter(|(choice, digest)| {
                resolve(path, choice, mode.clone(), &snapshot.layout.charset)
                    .is_ok_and(|old| &old.digest == digest)
            })
            .or(r.previous)
    });
    let receipt = Receipt {
        active: Some(snapshot.choice.clone()),
        digest: snapshot.digest.clone(),
        previous,
    };
    crate::data::atomic_write(
        &target,
        &serde_json::to_vec(&receipt).map_err(|e| Error::Invalid(e.to_string()))?,
    )?;
    Ok(())
}
pub fn restore(path: &Path, mode: Mode, charset: &str) -> Result<Snapshot, Error> {
    let bytes = std::fs::read(path.with_file_name("keyboard-selection.json"))?;
    let receipt: Receipt =
        serde_json::from_slice(&bytes).map_err(|e| Error::Invalid(e.to_string()))?;
    let resolve_checked = |(choice, digest): (Choice, String)| -> Result<Snapshot, Error> {
        let snapshot = resolve(path, &choice, mode.clone(), charset)?;
        if snapshot.digest != digest {
            return Err(Error::Invalid("组合资源摘要发生变化".into()));
        }
        Ok(snapshot)
    };
    resolve_checked((
        receipt
            .active
            .ok_or_else(|| Error::Invalid("当前未启用自定义布局".into()))?,
        receipt.digest,
    ))
    .or_else(|error| receipt.previous.map_or(Err(error), resolve_checked))
}

pub fn current_choice(path: &Path) -> Result<Option<Choice>, Error> {
    match std::fs::read(path.with_file_name("keyboard-selection.json")) {
        Ok(bytes) => Ok(serde_json::from_slice::<Receipt>(&bytes)
            .map_err(|e| Error::Invalid(e.to_string()))?
            .active),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn skin_wire(snapshot: &runtime::Snapshot) -> serde_json::Value {
    use base64::Engine;
    let mut images = serde_json::Map::new();
    if let Some(package) = &snapshot.package {
        for (role, image) in &snapshot.images {
            images.insert(role.clone(),serde_json::json!({"style":image.style,"path":image.path,"width":image.width,"height":image.height,
                "png":base64::engine::general_purpose::STANDARD.encode(&package.files[&image.path])}));
        }
    }
    serde_json::json!({"parameters":snapshot.parameters.as_ref(),"images":images,"warnings":snapshot.warnings,"legacy":snapshot.legacy})
}
pub fn wire(snapshot: &Snapshot) -> serde_json::Value {
    serde_json::json!({"choice":snapshot.choice,"layout":snapshot.layout,"mapping":snapshot.mapping,
        "mapping_identity":snapshot.mapping_identity,"definition":snapshot.definition(),"revision":snapshot.revision,"digest":snapshot.digest,
        "resource_digests":snapshot.resource_digests,"role_fallbacks":snapshot.role_fallbacks,
        "skin":{"light":skin_wire(&snapshot.light),"dark":skin_wire(&snapshot.dark)}})
}

#[derive(Deserialize)]
#[serde(tag = "action", deny_unknown_fields)]
pub enum Request {
    List {
        path: std::path::PathBuf,
    },
    Prepare {
        path: std::path::PathBuf,
        choice: Choice,
        mode: Mode,
        charset: String,
    },
    Save {
        path: std::path::PathBuf,
        choice: Choice,
        mode: Mode,
        charset: String,
        digest: String,
    },
    Restore {
        path: std::path::PathBuf,
        mode: Mode,
        charset: String,
    },
    Reset {
        path: std::path::PathBuf,
    },
}
pub fn execute(request: Request) -> Result<serde_json::Value, Error> {
    use serde_json::json;
    match request {
        Request::Prepare {
            path,
            choice,
            mode,
            charset,
        } => Ok(wire(&resolve(&path, &choice, mode, &charset)?)),
        Request::Save {
            path,
            choice,
            mode,
            charset,
            digest,
        } => {
            let snapshot = resolve(&path, &choice, mode, &charset)?;
            if snapshot.digest != digest {
                return Err(Error::Invalid("待应用组合已变化，请重新预览".into()));
            }
            save(&path, &snapshot)?;
            Ok(json!({"saved":true,"revision":snapshot.revision}))
        }
        Request::Restore {
            path,
            mode,
            charset,
        } => {
            if current_choice(&path)?.is_none() {
                return Ok(serde_json::Value::Null);
            }
            Ok(wire(&restore(&path, mode, &charset)?))
        }
        Request::Reset { path } => {
            let receipt = Receipt {
                active: None,
                digest: String::new(),
                previous: None,
            };
            crate::data::atomic_write(
                &path.with_file_name("keyboard-selection.json"),
                &serde_json::to_vec(&receipt).map_err(|e| Error::Invalid(e.to_string()))?,
            )?;
            Ok(json!({"saved":true}))
        }
        Request::List { path } => {
            let mut items = Vec::new();
            let store = runtime::store_path(&path);
            if store.exists() {
                let mut paths = std::fs::read_dir(store)?
                    .map(|e| e.map(|e| e.path()))
                    .collect::<Result<Vec<_>, _>>()?;
                paths.sort();
                for path in paths
                    .into_iter()
                    .filter(|p| p.extension().is_some_and(|e| e == "mywubi-skin"))
                {
                    let package = match Package::open(&path) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let Ok(states) = keyboard::compatibility(&package, &supported()) else {
                        continue;
                    };
                    for c in package.platforms["android"]
                        .components
                        .iter()
                        .filter(|c| c.kind == "layout")
                    {
                        let checked =
                            keyboard::layout(&package, c).and_then(|(layout, mapping)| {
                                layout.validate_geometry(
                                    mapping.as_ref(),
                                    &[KeyAction::Space, KeyAction::Enter, KeyAction::Cancel],
                                )?;
                                Ok(layout)
                            });
                        let reason = if !states[&c.id].enabled {
                            Some(states[&c.id].reasons.join("；"))
                        } else {
                            checked.as_ref().err().map(ToString::to_string)
                        };
                        items.push(json!({"selection":{"id":package.metadata.id,"version":package.metadata.version,"component":c.id},"name":format!("{} / {}",package.metadata.name,c.id),"enabled":reason.is_none(),"reason":reason,"modes":checked.as_ref().ok().map(|l|&l.modes)}));
                    }
                }
            }
            Ok(json!({"items":items,"choice":current_choice(&path)?}))
        }
    }
}
