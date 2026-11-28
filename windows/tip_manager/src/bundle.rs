//! Install both in-process text services on 64-bit Windows.
use crate::TipManagerError;
use std::path::Path;

pub const X86_DLL: &str = "im_engine_x86.dll";

pub fn install_bundle(dll: &str) -> Result<(), TipManagerError> {
    #[cfg(target_arch = "x86_64")]
    register_x86(&Path::new(dll).with_file_name(X86_DLL), false)?;
    crate::install(dll)
}

pub fn uninstall_bundle(_dll: &Path) -> Result<(), TipManagerError> {
    #[cfg(target_arch = "x86_64")]
    {
        use windows::Win32::System::Registry::{KEY_READ, KEY_WOW64_32KEY};
        if crate::detect::is_registry_present(KEY_READ | KEY_WOW64_32KEY) {
            register_x86(&_dll.with_file_name(X86_DLL), true)?;
        }
    }
    crate::uninstall()
}

#[cfg(target_arch = "x86_64")]
fn register_x86(dll: &Path, uninstall: bool) -> Result<(), TipManagerError> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::process::Command;
    use windows::Win32::System::SystemInformation::GetSystemWow64DirectoryW;

    if !dll.is_absolute() || !dll.is_file() {
        return Err(TipManagerError::DllNotFound(dll.to_owned()));
    }
    let mut directory = [0u16; 260];
    let len = unsafe { GetSystemWow64DirectoryW(Some(&mut directory)) } as usize;
    if len == 0 || len >= directory.len() {
        return Err(TipManagerError::Com(
            "无法定位 Windows 32 位系统目录".into(),
        ));
    }
    let path = std::path::PathBuf::from(OsString::from_wide(&directory[..len]));
    let mut command = Command::new(path.join("regsvr32.exe"));
    command.arg("/s");
    if uninstall {
        command.arg("/u");
    }
    let status = command
        .arg(dll)
        .status()
        .map_err(|e| TipManagerError::Com(e.to_string()))?;
    if !status.success() {
        return Err(TipManagerError::Com(format!(
            "32 位输入法注册工具退出码 {status}"
        )));
    }
    Ok(())
}
