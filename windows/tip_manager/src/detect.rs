//! TIP 状态检测。
//!
//! 通过注册表查询 + COM IsEnabledProfile 判断当前 TIP 安装与启用状态。

use windows::core::HSTRING;
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, HKEY, HKEY_CLASSES_ROOT, KEY_READ,
};

use crate::guids::clsid_string;
use crate::profile::TipProfileManager;
use crate::TipStatus;

const ERROR_SUCCESS: WIN32_ERROR = WIN32_ERROR(0);

/// 检查注册表中 TIP CLSID 键是否存在。
fn is_registry_present() -> bool {
    let clsid_str = clsid_string();
    let inproc_path = HSTRING::from(format!("CLSID\\{clsid_str}\\InprocServer32"));
    let mut key = HKEY::default();
    let status =
        unsafe { RegOpenKeyExW(HKEY_CLASSES_ROOT, &inproc_path, None, KEY_READ, &mut key) };
    if status == ERROR_SUCCESS {
        unsafe {
            let _ = RegCloseKey(key);
        };
        true
    } else {
        false
    }
}

/// 检测 TIP 状态。
///
/// `profile_mgr` 参数允许注入 mock 用于测试。
/// 若传 `None`，内部创建真实 `ComProfileManager`。
pub fn detect_status_impl(profile_mgr: Option<&dyn TipProfileManager>) -> TipStatus {
    if !is_registry_present() {
        return TipStatus::NotInstalled;
    }

    let mgr: Box<dyn TipProfileManager + '_>;
    let pm: &dyn TipProfileManager = if let Some(m) = profile_mgr {
        m
    } else {
        match crate::profile::ComProfileManager::new() {
            Ok(m) => {
                mgr = Box::new(m);
                mgr.as_ref()
            }
            Err(e) => {
                log::warn!("[tip_manager] COM 不可用，状态标记为 Unknown: {e}");
                return TipStatus::Unknown;
            }
        }
    };

    match pm.is_enabled() {
        Ok(true) => TipStatus::InstalledEnabled,
        Ok(false) => TipStatus::InstalledDisabled,
        Err(e) => {
            log::warn!("[tip_manager] IsEnabledProfile 失败: {e}");
            TipStatus::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::mock::MockProfileManager;

    #[test]
    fn test_detect_enabled_via_mock() {
        let mock = MockProfileManager::new(true, true);
        let _ = detect_status_impl(Some(&mock));
    }

    #[test]
    fn test_detect_disabled_via_mock() {
        let mock = MockProfileManager::new(false, true);
        let _ = detect_status_impl(Some(&mock));
    }

    #[test]
    fn test_detect_com_failure_via_mock() {
        let mock = MockProfileManager::new(false, true);
        mock.fail_next.set(true);
        let _ = detect_status_impl(Some(&mock));
    }
}
