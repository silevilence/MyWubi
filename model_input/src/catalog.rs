//! User-owned package catalog. No model loading or native code runs here.
use crate::package::{self, Manifest, PackageError};
use core_engine::{config::ModelSelection, config_path::model_store_path, Config};
use std::path::{Path, PathBuf};

pub struct Catalog {
    pub root: PathBuf,
}
impl Catalog {
    pub fn for_config(config: &Path) -> Self {
        Self {
            root: model_store_path(config),
        }
    }
    pub fn list(&self) -> Result<Vec<Manifest>, PackageError> {
        package::list(&self.root)?
            .iter()
            .map(|p| package::validate(p).map(|p| p.manifest))
            .collect()
    }
    pub fn import(&self, source: &Path) -> Result<Manifest, PackageError> {
        package::validate(&package::import(source, &self.root)?).map(|p| p.manifest)
    }
    pub fn remove(&self, selection: &ModelSelection, config: &Path) -> Result<(), PackageError> {
        // Check the saved selection at the mutation boundary, not a stale UI draft.
        let config = Config::load(config).map_err(|_| PackageError::Manifest)?;
        let selected = config
            .prediction
            .model
            .as_ref()
            .and_then(ModelSelection::directory_name)
            .map(|name| self.root.join(name));
        package::remove(
            &self.root,
            &selection.id,
            &selection.version,
            selected.as_deref(),
        )
    }
}

/// Separate management transport; never acquires the core input-session registry.
pub fn exchange(bytes: &[u8]) -> Vec<u8> {
    #[derive(serde::Deserialize)]
    #[serde(tag = "op", deny_unknown_fields)]
    enum Request {
        List {
            config: PathBuf,
        },
        Import {
            config: PathBuf,
            source: PathBuf,
        },
        Remove {
            config: PathBuf,
            selection: ModelSelection,
        },
    }
    let result = (|| -> Result<serde_json::Value, String> {
        if bytes.len() > 64 * 1024 {
            return Err("request exceeds limit".into());
        }
        let request: Request =
            serde_json::from_slice(bytes).map_err(|_| "invalid catalog request")?;
        let config = match &request {
            Request::List { config }
            | Request::Import { config, .. }
            | Request::Remove { config, .. } => config,
        };
        if !config.is_absolute() {
            return Err("configuration path must be absolute".into());
        }
        let catalog = Catalog::for_config(config);
        match &request {
            Request::List { .. } => {}
            Request::Import { source, .. } => {
                catalog.import(source).map_err(|e| e.to_string())?;
            }
            Request::Remove { selection, .. } => catalog
                .remove(selection, config)
                .map_err(|e| e.to_string())?,
        }
        serde_json::to_value(catalog.list().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    })();
    serde_json::to_vec(&match result {
        Ok(value) => serde_json::json!({"ok":true,"value":value}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_and_model_location_survive_application_version_changes() {
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("user");
        let old =
            core_engine::config_path::resolve_config_path_from(&root.path().join("app-v1"), &user)
                .unwrap();
        let models = Catalog::for_config(&old.path);
        std::fs::create_dir_all(&models.root).unwrap();
        std::fs::write(models.root.join("user-owned-marker"), b"preserve").unwrap();
        let mut config = Config::load(&old.path).unwrap();
        config.prediction.enabled = true;
        config.save(&old.path).unwrap();
        let next =
            core_engine::config_path::resolve_config_path_from(&root.path().join("app-v2"), &user)
                .unwrap();
        assert_eq!(models.root, Catalog::for_config(&next.path).root);
        assert_eq!(
            std::fs::read(models.root.join("user-owned-marker")).unwrap(),
            b"preserve"
        );
        assert!(Config::load(next.path).unwrap().prediction.enabled);
    }
}
