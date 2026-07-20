//! ITfInputProcessorProfileMgr / ITfInputProcessorProfiles COM 封装。
//!
//! 提供 `TipProfileManager` trait 及其 COM 实现，用于 TIP Profile 的
//! 注册/反注册/启用/禁用操作。

use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::{
    CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles, ITfCategoryMgr,
    ITfInputProcessorProfileMgr, ITfInputProcessorProfiles, GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
    GUID_TFCAT_TIPCAP_COMLESS, GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
    GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT, GUID_TFCAT_TIPCAP_SECUREMODE,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT, GUID_TFCAT_TIPCAP_UIELEMENTENABLED, GUID_TFCAT_TIP_KEYBOARD,
};
use windows_core::Interface;

use crate::error::TipManagerError;
use crate::guids::{CLSID_TEXT_SERVICE, GUID_PROFILE, TEXT_SERVICE_NAME};

/// HKL 零值（无替代键盘布局）。
const NULL_HKL: HKL = HKL(std::ptr::null_mut());

/// CLSID_TF_InputProcessorProfileMgr（windows-rs 0.61 未导出此常量）。
///
/// 与 `CLSID_TF_InputProcessorProfiles` 指向同一 COM 类，
/// 但用于获取 `ITfInputProcessorProfileMgr` 接口。
#[allow(dead_code)]
const CLSID_TF_INPUTPROCESSORPROFILEMGR: windows::core::GUID =
    windows::core::GUID::from_u128(0x71c6e74d_0f28_11d8_a82a_00065b84435c);

/// 抽象 TIP Profile 管理接口，方便测试 mock。
pub trait TipProfileManager {
    fn is_enabled(&self) -> Result<bool, TipManagerError>;
    fn enable(&self) -> Result<(), TipManagerError>;
    fn disable(&self) -> Result<(), TipManagerError>;
    fn register_profile(&self) -> Result<(), TipManagerError>;
    fn unregister_profile(&self) -> Result<(), TipManagerError>;
}

/// 真实 COM 实现。
///
/// 内部持有两个接口指针（来自同一 COM 对象）：
/// - `profiles` (`ITfInputProcessorProfiles`) — 用于启用/禁用/状态查询
/// - `mgr` (`ITfInputProcessorProfileMgr`) — 用于注册/反注册 Profile
pub struct ComProfileManager {
    /// ITfInputProcessorProfiles（启用/禁用/状态查询）。
    #[allow(dead_code)]
    profiles: ITfInputProcessorProfiles,
    /// ITfInputProcessorProfileMgr（注册/反注册 Profile）。
    mgr: ITfInputProcessorProfileMgr,
    /// 简中语言 ID。
    lang_id: u16,
}

impl ComProfileManager {
    /// 创建并初始化 COM 接口实例。
    pub fn new() -> Result<Self, TipManagerError> {
        // 先以 CLSID_TF_InputProcessorProfiles 创建 COM 对象，
        // 它同时实现了 ITfInputProcessorProfiles 和 ITfInputProcessorProfileMgr。
        let profiles: ITfInputProcessorProfiles = unsafe {
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
        }
        .map_err(|e| {
            TipManagerError::Com(format!(
                "CoCreateInstance(CLSID_TF_InputProcessorProfiles) 失败: {e}"
            ))
        })?;

        // 从同一对象查询 ITfInputProcessorProfileMgr 接口。
        let mgr: ITfInputProcessorProfileMgr = profiles.cast().map_err(|e| {
            TipManagerError::Com(format!(
                "QueryInterface(ITfInputProcessorProfileMgr) 失败: {e}"
            ))
        })?;

        Ok(Self {
            profiles,
            mgr,
            lang_id: 0x0804, // 简体中文 LANGID
        })
    }
}

impl TipProfileManager for ComProfileManager {
    fn is_enabled(&self) -> Result<bool, TipManagerError> {
        let result = unsafe {
            self.profiles
                .IsEnabledLanguageProfile(&CLSID_TEXT_SERVICE, self.lang_id, &GUID_PROFILE)
        }
        .map_err(|e| TipManagerError::Com(format!("IsEnabledLanguageProfile 失败: {e}")))?;
        Ok(result.as_bool())
    }

    fn enable(&self) -> Result<(), TipManagerError> {
        unsafe {
            self.profiles.EnableLanguageProfile(
                &CLSID_TEXT_SERVICE,
                self.lang_id,
                &GUID_PROFILE,
                true,
            )
        }
        .map_err(|e| TipManagerError::Com(format!("EnableLanguageProfile 失败: {e}")))
    }

    fn disable(&self) -> Result<(), TipManagerError> {
        unsafe {
            self.profiles.EnableLanguageProfile(
                &CLSID_TEXT_SERVICE,
                self.lang_id,
                &GUID_PROFILE,
                false,
            )
        }
        .map_err(|e| TipManagerError::Com(format!("DisableLanguageProfile 失败: {e}")))
    }

    fn register_profile(&self) -> Result<(), TipManagerError> {
        let desc: Vec<u16> = TEXT_SERVICE_NAME
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            self.mgr.RegisterProfile(
                &CLSID_TEXT_SERVICE,
                self.lang_id,
                &GUID_PROFILE,
                &desc,
                &[],      // pchIconFile — 空，注册表中已有 IconFile
                0,        // uiconindex
                NULL_HKL, // hklSubstitute — 零值表示无替代布局
                0,        // dwPreferredLayout
                true,     // bEnabledByDefault
                0,        // dwFlags
            )
        }
        .map_err(|e| TipManagerError::Com(format!("RegisterProfile 失败: {e}")))
    }

    fn unregister_profile(&self) -> Result<(), TipManagerError> {
        unsafe {
            self.mgr.UnregisterProfile(
                &CLSID_TEXT_SERVICE,
                self.lang_id,
                &GUID_PROFILE,
                0, // dwFlags
            )
        }
        .map_err(|e| TipManagerError::Com(format!("UnregisterProfile 失败: {e}")))
    }
}

/// TSF 标准类别列表（参考 ime-rs / Windows SDK）。
const SUPPORT_CATEGORIES: [windows::core::GUID; 8] = [
    GUID_TFCAT_TIP_KEYBOARD,
    GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
    GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
    GUID_TFCAT_TIPCAP_SECUREMODE,
    GUID_TFCAT_TIPCAP_COMLESS,
    GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT,
    GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
];

/// 通过 `ITfCategoryMgr::RegisterCategory` 注册所有 TSF 类别。
pub fn register_categories() -> Result<(), TipManagerError> {
    let category_manager: ITfCategoryMgr =
        unsafe { CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER) }.map_err(
            |e| TipManagerError::Com(format!("CoCreateInstance(ITfCategoryMgr) 失败: {e}")),
        )?;

    for guid in SUPPORT_CATEGORIES {
        unsafe {
            category_manager.RegisterCategory(&CLSID_TEXT_SERVICE, &guid, &CLSID_TEXT_SERVICE)
        }
        .map_err(|e| TipManagerError::Com(format!("RegisterCategory 失败: {e:?}")))?;
    }
    log::info!(
        "[tip_manager] 已注册 {} 个 TSF 类别",
        SUPPORT_CATEGORIES.len()
    );
    Ok(())
}

/// 通过 `ITfCategoryMgr::UnregisterCategory` 反注册所有 TSF 类别。
pub fn unregister_categories() -> Result<(), TipManagerError> {
    let category_manager: ITfCategoryMgr =
        match unsafe { CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER) } {
            Ok(m) => m,
            Err(_) => {
                log::warn!("[tip_manager] ITfCategoryMgr 不可用，跳过类别清理");
                return Ok(());
            }
        };

    for guid in SUPPORT_CATEGORIES {
        let _ = unsafe {
            category_manager.UnregisterCategory(&CLSID_TEXT_SERVICE, &guid, &CLSID_TEXT_SERVICE)
        };
    }
    log::info!(
        "[tip_manager] 已清理 {} 个 TSF 类别",
        SUPPORT_CATEGORIES.len()
    );
    Ok(())
}

/// Mock 实现（测试用）。
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::cell::Cell;

    pub struct MockProfileManager {
        pub enabled: Cell<bool>,
        pub registered: Cell<bool>,
        pub fail_next: Cell<bool>,
    }

    impl MockProfileManager {
        pub fn new(enabled: bool, registered: bool) -> Self {
            Self {
                enabled: Cell::new(enabled),
                registered: Cell::new(registered),
                fail_next: Cell::new(false),
            }
        }
    }

    impl TipProfileManager for MockProfileManager {
        fn is_enabled(&self) -> Result<bool, TipManagerError> {
            if self.fail_next.get() {
                return Err(TipManagerError::Com("mock failure".into()));
            }
            Ok(self.enabled.get())
        }

        fn enable(&self) -> Result<(), TipManagerError> {
            if self.fail_next.get() {
                return Err(TipManagerError::Com("mock failure".into()));
            }
            self.enabled.set(true);
            Ok(())
        }

        fn disable(&self) -> Result<(), TipManagerError> {
            if self.fail_next.get() {
                return Err(TipManagerError::Com("mock failure".into()));
            }
            self.enabled.set(false);
            Ok(())
        }

        fn register_profile(&self) -> Result<(), TipManagerError> {
            if self.fail_next.get() {
                return Err(TipManagerError::Com("mock failure".into()));
            }
            self.registered.set(true);
            self.enabled.set(true);
            Ok(())
        }

        fn unregister_profile(&self) -> Result<(), TipManagerError> {
            if self.fail_next.get() {
                return Err(TipManagerError::Com("mock failure".into()));
            }
            self.registered.set(false);
            Ok(())
        }
    }

    #[test]
    fn test_mock_enable_disable() {
        let m = MockProfileManager::new(false, true);
        assert!(!m.is_enabled().unwrap());
        m.enable().unwrap();
        assert!(m.is_enabled().unwrap());
        m.disable().unwrap();
        assert!(!m.is_enabled().unwrap());
    }

    #[test]
    fn test_mock_register_unregister() {
        let m = MockProfileManager::new(false, false);
        m.register_profile().unwrap();
        assert!(m.registered.get());
        assert!(m.enabled.get());
        m.unregister_profile().unwrap();
        assert!(!m.registered.get());
    }

    #[test]
    fn test_mock_failure() {
        let m = MockProfileManager::new(false, true);
        m.fail_next.set(true);
        assert!(m.is_enabled().is_err());
        m.fail_next.set(false);
        assert!(m.is_enabled().is_ok());
    }
}
