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
