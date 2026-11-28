use core_engine::skin::{archive, runtime, Package};
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::path::{Path, PathBuf};

/// Open one file, then install the same inspected contents only after consent.
fn confirm_import(
    path: &Path,
    confirm: impl FnOnce(&Package) -> bool,
    config: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<Option<String>, String> {
    let package = Package::open(path).map_err(|err| err.to_string())?;
    if !confirm(&package) {
        return Ok(None);
    }
    let result = archive::install_package(&runtime::store_path(&config()?), &package)
        .map_err(|err| err.to_string())?;
    let name = &package.metadata.name;
    let message = if result.already_present {
        format!("「{name}」已在皮肤库中。")
    } else {
        format!("已将「{name}」加入皮肤库。")
    };
    Ok(Some(message))
}

fn windows_choices(
    item: &serde_json::Value,
) -> Vec<(core_engine::skin::preferences::Selection, String)> {
    let Some(components) = item["components"]
        .as_array()
        .filter(|_| item["reason"].is_null())
    else {
        return Vec::new();
    };
    components
        .iter()
        .filter_map(|component| {
            if component["kind"] != "appearance" || component["enabled"] != true {
                return None;
            }
            let selection = core_engine::skin::preferences::Selection {
                id: item["id"].as_str()?.to_owned(),
                version: item["version"].as_str()?.to_owned(),
                component: component["id"].as_str()?.to_owned(),
            };
            let label = format!(
                "{} · {} / {}",
                item["name"].as_str()?,
                selection.version,
                selection.component
            );
            Some((selection, label))
        })
        .collect()
}

pub(crate) fn list_for_settings(
    config: &Path,
) -> Result<Vec<(core_engine::skin::preferences::Selection, String)>, String> {
    use core_engine::skin::manage::{execute, Request};
    let result = execute(Request::List {
        path: config.to_owned(),
        platform: "windows".into(),
    })
    .map_err(|e| e.to_string())?;
    Ok(result["items"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(windows_choices)
        .collect())
}

pub(crate) fn import_for_settings(source: &Path, config: &Path) -> Result<String, String> {
    use core_engine::skin::manage::{execute, Request};
    let result = execute(Request::Import {
        path: config.to_owned(),
        source: source.to_owned(),
        platform: "windows".into(),
    })
    .map_err(|e| e.to_string())?;
    let item = &result["item"];
    let name = item["name"].as_str().unwrap_or_default();
    Ok(if windows_choices(item).is_empty() {
        format!("「{name}」已在皮肤库中，此包没有可用于 Windows 的外观。")
    } else {
        format!("「{name}」已在皮肤库中，可在「皮肤方案」中选用。")
    })
}

pub(crate) fn validate_windows(config: &core_engine::Config, path: &Path) -> Result<(), String> {
    for dark in [false, true] {
        let skin =
            runtime::load(path, config, "windows", dark, false).map_err(|e| e.to_string())?;
        if let Some(package) = &skin.package {
            text_render::skin_images::decode(package, &skin.images).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn open(path: &Path) {
    let result = confirm_import(
        path,
        |package| {
            let platforms = package
                .platforms
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("、");
            MessageDialog::new()
                .set_title("MyWubi · 导入皮肤")
                .set_description(format!(
                    "将「{}」加入皮肤库？\n\n版本：{}\n适用平台：{}",
                    package.metadata.name, package.metadata.version, platforms
                ))
                .set_buttons(MessageButtons::OkCancel)
                .show()
                == MessageDialogResult::Ok
        },
        || {
            crate::config_path::resolve_config_path()
                .map(|(path, _, _)| path)
                .map_err(|err| err.to_string())
        },
    );
    let (level, message) = match result {
        Ok(None) => return,
        Ok(Some(message)) => (MessageLevel::Info, message),
        Err(error) => (MessageLevel::Error, format!("无法导入皮肤。\n{error}")),
    };
    MessageDialog::new()
        .set_level(level)
        .set_title("MyWubi · 导入皮肤")
        .set_description(message)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_can_select_two_imported_skins_then_return_to_a_preset() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config.toml");
        let mut settings = core_engine::Config::default();
        for preset in ["jade", "plain"] {
            let mut package = core_engine::skin::parameters::builtin_package(preset).unwrap();
            package.metadata.id = format!("custom.{preset}");
            let path = directory.path().join(format!("{preset}.mywubi-skin"));
            package.export_new(&path).unwrap();
            import_for_settings(&path, &config).unwrap();
        }
        let choices = list_for_settings(&config).unwrap();
        assert_eq!(choices.len(), 2);
        for (selection, _) in choices {
            settings.skins.windows.selection = Some(selection.clone());
            settings.save(&config).unwrap();
            let saved = core_engine::Config::load(&config).unwrap();
            assert_eq!(saved.skins.windows.selection.as_ref(), Some(&selection));
            let skin = runtime::load(&config, &saved, "windows", false, false).unwrap();
            assert_eq!(skin.package.unwrap().metadata.id, selection.id);
        }
        settings.skins.windows.selection = None;
        settings.skins.windows.preset = "plain".into();
        settings.save(&config).unwrap();
        assert!(core_engine::Config::load(&config)
            .unwrap()
            .skins
            .windows
            .selection
            .is_none());
    }

    #[test]
    fn confirm_import_uses_inspected_contents_and_cancel_never_creates_library() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("自制 皮肤.mywubi-skin");
        let config = dir.path().join("config.toml");
        let package = core_engine::skin::parameters::builtin_package("jade").unwrap();
        package.export_new(&file).unwrap();
        assert_eq!(
            confirm_import(&file, |_| false, || panic!("unexpected config write")).unwrap(),
            None
        );
        assert!(!runtime::store_path(&config).exists());
        confirm_import(
            &file,
            |inspected| {
                assert_eq!(inspected.metadata.id, package.metadata.id);
                std::fs::write(&file, b"source replaced after inspection").unwrap();
                true
            },
            || Ok(config.clone()),
        )
        .unwrap();
        let target = runtime::store_path(&config).join(format!(
            "{}@{}.mywubi-skin",
            package.metadata.id, package.metadata.version
        ));
        assert_eq!(
            Package::open(&target).unwrap().metadata.name,
            package.metadata.name
        );
        assert!(!config.exists());
    }
}
