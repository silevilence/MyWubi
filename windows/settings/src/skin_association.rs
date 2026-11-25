use std::{ffi::OsString, io, path::Path};
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use winreg::{
    enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_NONE},
    RegKey, RegValue,
};

const EXTENSION: &str = ".mywubi-skin";
const INSTALLED: &str = "MyWubi.Skin";
const PORTABLE: &str = "MyWubi.Portable.Skin";

fn open_command(exe: &Path) -> OsString {
    let mut value = OsString::from("\"");
    value.push(exe);
    value.push("\" --import-skin \"%1\"");
    value
}

fn optional<T>(value: io::Result<T>) -> io::Result<Option<T>> {
    match value {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn register(classes: &RegKey, exe: &Path, prog_id: &str) -> io::Result<()> {
    let (program, _) = classes.create_subkey(prog_id)?;
    program.set_value("", &"MyWubi 皮肤包")?;
    program
        .create_subkey("Application")?
        .0
        .set_value("ApplicationName", &"MyWubi 设置")?;
    program
        .create_subkey(r"shell\open\command")?
        .0
        .set_value("", &open_command(exe))?;
    let (extension, _) = classes.create_subkey(EXTENSION)?;
    extension
        .create_subkey("OpenWithProgids")?
        .0
        .set_raw_value(
            prog_id,
            &RegValue {
                vtype: REG_NONE,
                bytes: vec![],
            },
        )?;
    if optional(extension.get_raw_value(""))?.is_none() {
        extension.set_value("", &prog_id)?;
    }
    Ok(())
}

fn remove_empty(parent: &RegKey, name: &str) -> io::Result<()> {
    if let Some(key) = optional(parent.open_subkey(name))? {
        let metadata = key.query_info()?;
        if metadata.values == 0 && metadata.sub_keys == 0 {
            parent.delete_subkey(name)?;
        }
    }
    Ok(())
}

fn unregister(classes: &RegKey, exe: &Path, prog_id: &str) -> io::Result<()> {
    let command = optional(
        classes
            .open_subkey(format!(r"{prog_id}\shell\open\command"))
            .and_then(|key| key.get_value::<OsString, _>("")),
    )?;
    if command != Some(open_command(exe)) {
        return Ok(());
    }
    if let Some(extension) =
        optional(classes.open_subkey_with_flags(EXTENSION, KEY_READ | KEY_WRITE))?
    {
        if optional(extension.get_value::<String, _>(""))?.as_deref() == Some(prog_id) {
            extension.delete_value("")?;
        }
        if let Some(choices) =
            optional(extension.open_subkey_with_flags("OpenWithProgids", KEY_WRITE))?
        {
            optional(choices.delete_value(prog_id))?;
            drop(choices);
            remove_empty(&extension, "OpenWithProgids")?;
        }
        drop(extension);
        remove_empty(classes, EXTENSION)?;
    }
    classes.delete_subkey_all(prog_id)
}

pub fn set_registered(portable: bool, enable: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let classes = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"Software\Classes", KEY_READ | KEY_WRITE)?;
    let prog_id = if portable { PORTABLE } else { INSTALLED };
    if enable {
        register(&classes, &exe, prog_id)?;
    } else {
        unregister(&classes, &exe, prog_id)?;
    }
    // Notify Explorer after the complete association is visible.
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrade_repairs_quoted_paths_and_uninstall_respects_other_handlers() {
        let scratch = tempfile::tempdir().unwrap();
        let key = format!(
            r"Software\MyWubi-Association-Test-{}",
            scratch.path().file_name().unwrap().to_string_lossy()
        );
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let (classes, _) = user.create_subkey(&key).unwrap();
        let old = Path::new(r"C:\My Wubi\旧\settings.exe");
        let new = Path::new(r"C:\My Wubi\新\settings.exe");
        register(&classes, old, INSTALLED).unwrap();
        register(&classes, new, INSTALLED).unwrap();
        unregister(&classes, old, INSTALLED).unwrap();
        let command: String = classes
            .open_subkey(r"MyWubi.Skin\shell\open\command")
            .unwrap()
            .get_value("")
            .unwrap();
        assert_eq!(
            command,
            r#""C:\My Wubi\新\settings.exe" --import-skin "%1""#
        );
        let ext = classes
            .open_subkey_with_flags(EXTENSION, KEY_READ | KEY_WRITE)
            .unwrap();
        assert_eq!(ext.get_value::<String, _>("").unwrap(), INSTALLED);
        ext.set_value("", &"Other.Handler").unwrap();
        register(&classes, old, PORTABLE).unwrap();
        unregister(&classes, new, INSTALLED).unwrap();
        assert_eq!(ext.get_value::<String, _>("").unwrap(), "Other.Handler");
        assert!(classes.open_subkey(INSTALLED).is_err());
        assert!(ext
            .open_subkey("OpenWithProgids")
            .unwrap()
            .get_raw_value(PORTABLE)
            .is_ok());
        drop(ext);
        drop(classes);
        user.delete_subkey_all(key).unwrap();
    }
}
