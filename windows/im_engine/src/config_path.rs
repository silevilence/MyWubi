//! TSF 宿主中的配置路径：包进程也必须读取桌面设置程序的同一份配置。

use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DONT_VERIFY,
    KF_FLAG_NO_PACKAGE_REDIRECTION,
};

fn roaming_app_data() -> Result<PathBuf, String> {
    // SearchApp 等 AppContainer 无权枚举 Roaming 根目录；默认验证会失败。
    // 不重定向到包私有目录，实际文件读取仍由 Windows ACL 检查。
    unsafe {
        let path = SHGetKnownFolderPath(
            &FOLDERID_RoamingAppData,
            KF_FLAG_DONT_VERIFY | KF_FLAG_NO_PACKAGE_REDIRECTION,
            None,
        )
        .map_err(|error| format!("无法获取 AppData 路径: {error}"))?;
        let result = PathBuf::from(std::ffi::OsString::from_wide(path.as_wide()));
        CoTaskMemFree(Some(path.0.cast()));
        Ok(result)
    }
}

pub(crate) fn resolve(exe_dir: &Path) -> Result<PathBuf, String> {
    // 便携模式不依赖宿主能否查询用户目录。
    if core_engine::config_path::is_portable_mode(exe_dir) {
        return Ok(exe_dir.join("config.toml"));
    }
    let app_dir = roaming_app_data()?.join("MyWubi");
    core_engine::config_path::resolve_config_path_from(exe_dir, &app_dir)
        .map(|resolved| resolved.path)
        .map_err(|error| format!("定位配置路径失败: {error}"))
}
