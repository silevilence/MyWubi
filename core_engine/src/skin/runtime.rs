//! Publication of whole appearance snapshots, independent of dictionaries and sessions.
use super::{
    parameters::{self, Parameters},
    preferences::Preference,
    Error, Extensions, Package,
};
use crate::Config;
use arc_swap::ArcSwap;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug)]
pub struct Snapshot {
    pub parameters: Arc<Parameters>,
    pub source: Option<PathBuf>,
    pub package: Option<Arc<Package>>,
    pub warnings: Vec<String>,
    pub legacy: bool,
}

pub struct SkinRuntime {
    active: ArcSwap<Snapshot>,
}

impl SkinRuntime {
    pub fn new(snapshot: Snapshot) -> Self {
        Self {
            active: ArcSwap::from_pointee(snapshot),
        }
    }
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.active.load_full()
    }
    /// A failed refresh does not replace even part of the previous appearance/resources.
    pub fn refresh(
        &self,
        path: &Path,
        config: &Config,
        platform: &str,
        dark: bool,
        high_contrast: bool,
    ) -> Result<(), Error> {
        let next = load(path, config, platform, dark, high_contrast)?;
        self.active.store(Arc::new(next));
        Ok(())
    }
}

pub fn store_path(config: &Path) -> PathBuf {
    config.parent().unwrap_or(Path::new(".")).join("skins")
}

pub fn selection_path(config: &Path, preference: &Preference) -> Option<PathBuf> {
    preference
        .selection
        .as_ref()
        .map(|s| store_path(config).join(format!("{}@{}.mywubi-skin", s.id, s.version)))
}

pub fn load(
    path: &Path,
    config: &Config,
    platform: &str,
    system_dark: bool,
    high_contrast: bool,
) -> Result<Snapshot, Error> {
    let preference = match platform {
        "windows" => &config.skins.windows,
        "android" => &config.skins.android,
        _ => return Err(Error::Invalid("未知运行平台".into())),
    };
    preference.validate()?;
    let dark = preference.dark(system_dark);
    let source = selection_path(path, preference);
    let mut overrides = preference.overrides.clone();
    if !preference.use_skin_metrics {
        if platform == "windows" {
            overrides
                .entry("font_size".into())
                .or_insert(i64::from(config.appearance.font_size).into());
        } else {
            overrides.entry("key_text_size".into()).or_insert(20.into());
        }
    }
    let mut warnings = Vec::new();
    let mut package = None;
    let legacy = preference.selection.is_none() && preference.preset == "legacy";
    let mut parameters = if let (Some(source), Some(selection)) = (&source, &preference.selection) {
        let loaded = Package::open(source)?;
        if loaded.metadata.id != selection.id || loaded.metadata.version != selection.version {
            return Err(Error::Invalid("安装皮肤身份与选择版本不一致".into()));
        }
        let supported: BTreeSet<_> = [
            "container.zip/1".into(),
            format!("{platform}.base/1"),
            "appearance.parameters/1".into(),
        ]
        .into();
        let resolved = parameters::resolve(
            &loaded,
            platform,
            &selection.component,
            dark,
            &overrides,
            high_contrast,
            &supported,
        )?;
        warnings = resolved.warnings;
        package = Some(Arc::new(loaded));
        resolved.parameters
    } else if legacy {
        if platform == "windows" {
            parameters::legacy(&config.appearance)?
        } else {
            parameters::builtin("jade", dark)?
        }
    } else {
        parameters::builtin(&preference.preset, dark)?
    };
    if package.is_none() {
        parameters.overlay(&overrides)?;
        if high_contrast {
            let contrast = parameters::builtin("contrast", dark)?;
            let constraint: Extensions = contrast
                .0
                .into_iter()
                .filter(|(k, _)| {
                    parameters::COLORS.contains(&k.as_str())
                        || matches!(k.as_str(), "shadow" | "border_width")
                })
                .collect();
            parameters.overlay(&constraint)?;
        }
    }
    Ok(Snapshot {
        parameters: Arc::new(parameters),
        source,
        package,
        warnings,
        legacy,
    })
}

/// Background reload fast path: appearance changes do not invalidate engine identity.
pub fn same_input_configuration(a: &Config, b: &Config) -> bool {
    let (Ok(mut a), Ok(mut b)) = (serde_json::to_value(a), serde_json::to_value(b)) else {
        return false;
    };
    for value in [&mut a, &mut b] {
        if let Some(object) = value.as_object_mut() {
            object.remove("appearance");
            object.remove("skins");
        }
    }
    a == b
}
