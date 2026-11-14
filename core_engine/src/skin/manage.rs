//! Settings-only catalog operations; none of these select or activate a skin.
use super::{archive, parameters, preferences::Selection, runtime, Error, Package};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs::File, path::PathBuf};

#[derive(Deserialize)]
#[serde(tag = "action", deny_unknown_fields)]
pub enum Request {
    List {
        path: PathBuf,
        platform: String,
    },
    Import {
        path: PathBuf,
        source: PathBuf,
        platform: String,
    },
    Export {
        path: PathBuf,
        selection: Selection,
        target: PathBuf,
    },
}

fn describe(package: &Package, platform: &str) -> Value {
    let mut supported: BTreeSet<_> = [
        "container.zip/1".into(),
        format!("{platform}.base/1"),
        "appearance.parameters/1".into(),
    ]
    .into();
    if platform == "windows" { supported.insert(super::images::CAPABILITY.into()); }
    let compatibility = package.compatibility(platform, &supported, &["appearance".into()].into());
    let mut components = Vec::new();
    let mut reason = None;
    match compatibility {
        Ok((base, states)) => {
            if !base.enabled {
                reason = Some(base.reasons.join("；"));
            }
            for component in &package.platforms[platform].components {
                let state = &states[&component.id];
                let resolution = if state.enabled {
                    [false, true].into_iter().try_for_each(|dark| {
                        parameters::resolve(
                            package,
                            platform,
                            &component.id,
                            dark,
                            &Default::default(),
                            false,
                            &supported,
                        )
                        .map(|_| ())
                    })
                } else {
                    Err(Error::Invalid(state.reasons.join("；")))
                };
                components.push(json!({"id":component.id,"enabled":resolution.is_ok(),"reason":resolution.err().map(|e|e.to_string())}));
            }
        }
        Err(e) => reason = Some(e.to_string()),
    }
    json!({"id":package.metadata.id,"version":package.metadata.version,"name":package.metadata.name,
        "platforms":package.platforms.keys().collect::<Vec<_>>(),"components":components,"reason":reason})
}

pub fn execute(request: Request) -> Result<Value, Error> {
    match request {
        Request::List { path, platform } => {
            let store = runtime::store_path(&path);
            if !store.exists() {
                return Ok(json!({"items":[]}));
            }
            let mut paths = std::fs::read_dir(store)?
                .map(|e| e.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()?;
            paths.sort();
            let items: Vec<_> = paths.iter().filter(|p|p.extension().is_some_and(|e|e == "mywubi-skin")).map(|p| match Package::open(p) {
                Ok(package) => describe(&package, &platform),
                Err(e) => json!({"name":p.file_name().unwrap_or_default().to_string_lossy(),"reason":e.to_string(),"components":[]}),
            }).collect();
            Ok(json!({"items":items}))
        }
        Request::Import {
            path,
            source,
            platform,
        } => {
            let imported = archive::import(&runtime::store_path(&path), File::open(source)?)?;
            Ok(
                json!({"item":describe(&Package::open(&imported.path)?, &platform),"already_present":imported.already_present}),
            )
        }
        Request::Export {
            path,
            selection,
            target,
        } => {
            let preference = super::preferences::Preference {
                selection: Some(selection.clone()),
                ..Default::default()
            };
            preference.validate()?;
            let source = runtime::selection_path(&path, &preference)
                .ok_or_else(|| Error::Invalid("未选择皮肤".into()))?;
            let package = Package::open(&source)?;
            if package.metadata.id != selection.id || package.metadata.version != selection.version
            {
                return Err(Error::Invalid("安装皮肤身份不一致".into()));
            }
            package.export_new(&target)?;
            Ok(json!({"path":target}))
        }
    }
}
