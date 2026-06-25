//! 本输入法 TIP（Text Input Processor）跨进程复用的 COM 标识符。
//!
//! 集中维护 CLSID / IID 与显示名配置。`registrar.rs` 会写入注册表，
//! `factory.rs` 在 `DllGetClassObject` 中按 `CLSID_TEXT_SERVICE` 创建实例。

use windows::core::{GUID, HSTRING};

/// 输入法 TIP 主 COM 类的 CLSID。
///
/// 固定 GUID 不随版本变化，否则系统无法找到已注册的 TIP。
/// `0xC9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD8`
pub const CLSID_TEXT_SERVICE: GUID = GUID::from_u128(0xC9F2EAA4_0AB7_49C6_9F2C_8B8FA8D5FFD8);

/// 文本服务在注册表中的可读名（显示给用户）。
pub const TEXT_SERVICE_NAME: &str = "MyWubi 形码输入法";

/// 描述字段。
pub const TEXT_SERVICE_DESC: &str = "跨平台五笔/形码输入法 (TSF TIP)";

/// 进程内服务器相对路径（相对 DLL 安装目录）。
pub const IM_ENGINE_DLL: &str = "im_engine.dll";

/// TSF Profile GUID（唯一标识本输入法在系统中的配置组合）。
pub const GUID_PROFILE: GUID = GUID::from_u128(0xC9F2EAA4_0AB7_49C6_9F2C_8B8FA8D5FFD9);

/// TSF 键盘输入法类别（windows-rs 0.61 预定义值）。
pub const GUID_TFCAT_TIP_KEYBOARD: GUID =
    GUID::from_u128(0x34745C63_B2F0_4784_8B67_5E12C8701A31);

/// 让 `CLSID_TEXT_SERVICE` 的字符串表示可用于注册表键名。
pub fn clsid_string() -> String {
    format!("{{{:?}}}", CLSID_TEXT_SERVICE)
}

/// 用于注册表写入专用名（COM 要的是宽字符）。
pub fn text_service_name_wide() -> HSTRING {
    HSTRING::from(TEXT_SERVICE_NAME)
}

pub fn text_service_desc_wide() -> HSTRING {
    HSTRING::from(TEXT_SERVICE_DESC)
}

pub fn im_engine_dll_wide() -> HSTRING {
    HSTRING::from(IM_ENGINE_DLL)
}
