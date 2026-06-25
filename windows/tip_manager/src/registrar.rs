//! Windows 注册表的 TIP 注册与反注册实现。

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CLASSES_ROOT,
    HKEY_LOCAL_MACHINE, KEY_CREATE_SUB_KEY, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
    REG_DWORD, REG_SAM_FLAGS, REG_SZ,
};

use crate::error::TipManagerError;
use crate::guids::{clsid_string, TEXT_SERVICE_NAME, GUID_PROFILE};

const ERROR_SUCCESS: WIN32_ERROR = WIN32_ERROR(0);

/// 注册本 TIP。写入所有必要的注册表项。
pub fn register_tip(dll_path: &str) -> Result<(), TipManagerError> {
    let clsid_str = clsid_string();
    let clsid_wide = HSTRING::from(&clsid_str);
    let dll_wide = HSTRING::from(dll_path);

    // 1. HKCR\CLSID\{CLSID}
    let clsid_path = HSTRING::from(format!("CLSID\\{clsid_str}"));
    set_reg_sz(HKEY_CLASSES_ROOT, &clsid_path, PCWSTR::null(), &HSTRING::from(TEXT_SERVICE_NAME))?;

    // 2. InprocServer32
    let inproc_path = HSTRING::from(format!("CLSID\\{clsid_str}\\InprocServer32"));
    set_reg_sz(HKEY_CLASSES_ROOT, &inproc_path, PCWSTR::null(), &dll_wide)?;
    set_reg_sz(HKEY_CLASSES_ROOT, &inproc_path, w!("ThreadingModel"), &HSTRING::from("Apartment"))?;

    // 3. ProgID
    let progid_path = HSTRING::from(format!("CLSID\\{clsid_str}\\ProgID"));
    set_reg_sz(HKEY_CLASSES_ROOT, &progid_path, PCWSTR::null(), &HSTRING::from("MyWubi.TextService.1"))?;

    // 4. Implemented Categories\{CATID_TIP}
    let catid_tip = "{34745C63-B2F0-4784-8B67-5E12C8701A31}";
    let cat_path = HSTRING::from(format!("CLSID\\{clsid_str}\\Implemented Categories\\{catid_tip}"));
    set_reg_sz(HKEY_CLASSES_ROOT, &cat_path, PCWSTR::null(), &HSTRING::from(""))?;

    // 5. HKLM\SOFTWARE\Microsoft\CTF\TIP\{CLSID}
    let ctf_tip_path = format!("SOFTWARE\\Microsoft\\CTF\\TIP\\{clsid_str}");
    let ctf_tip_w = HSTRING::from(&ctf_tip_path);
    set_reg_sz(HKEY_LOCAL_MACHINE, &ctf_tip_w, PCWSTR::null(), &HSTRING::from(TEXT_SERVICE_NAME))?;

    // 6. LanguageProfile
    let profile_string = format!("{{{:?}}}", GUID_PROFILE);
    let lp_key_path = HSTRING::from(format!("{ctf_tip_path}\\LanguageProfile"));
    set_reg_sz(HKEY_LOCAL_MACHINE, &lp_key_path, PCWSTR::null(), &HSTRING::from(&profile_string))?;

    let lang_id = "0x00000804";
    let profile_path = HSTRING::from(format!(
        "{ctf_tip_path}\\LanguageProfile\\{lang_id}\\{profile_string}"
    ));
    set_reg_sz(HKEY_LOCAL_MACHINE, &profile_path, w!("Description"), &HSTRING::from(TEXT_SERVICE_NAME))?;
    set_reg_sz(HKEY_LOCAL_MACHINE, &profile_path, w!("IconFile"), &dll_wide)?;
    set_reg_dword(HKEY_LOCAL_MACHINE, &profile_path, w!("IconIndex"), 0)?;
    set_reg_dword(HKEY_LOCAL_MACHINE, &profile_path, w!("Enable"), 1)?;

    // 7. Display Description
    set_reg_sz(
        HKEY_LOCAL_MACHINE,
        &ctf_tip_w,
        w!("Display Description"),
        &HSTRING::from(TEXT_SERVICE_NAME),
    )?;

    // 8. EnableCompatibleTsf
    set_reg_dword(HKEY_LOCAL_MACHINE, &ctf_tip_w, w!("EnableCompatibleTsf"), 1)?;

    // 9. TIP Categories
    let cat_keyboard = "{3640E571-E878-4FE7-B341-35D393003EAB}";
    let cat_tip_path = HSTRING::from(format!("{ctf_tip_path}\\Category\\Category{catid_tip}"));
    let cat_kb_path = HSTRING::from(format!("{ctf_tip_path}\\Category\\Category{cat_keyboard}"));
    set_reg_sz(HKEY_LOCAL_MACHINE, &cat_tip_path, PCWSTR::null(), &HSTRING::from(""))?;
    set_reg_sz(HKEY_LOCAL_MACHINE, &cat_kb_path, PCWSTR::null(), &HSTRING::from(""))?;

    // 10. CLSID subkey
    set_reg_sz(
        HKEY_LOCAL_MACHINE,
        &HSTRING::from(format!("{ctf_tip_path}\\CLSID")),
        PCWSTR::null(),
        &clsid_wide,
    )?;

    log::info!("[tip_manager] register_tip: CLSID={clsid_str} dll={dll_path}");
    Ok(())
}

/// 反注册本 TIP。删除所有注册表项。
pub fn unregister_tip() -> Result<(), TipManagerError> {
    let clsid_str = clsid_string();

    let clsid_path = HSTRING::from(format!("CLSID\\{clsid_str}"));
    let hr = unsafe { RegDeleteTreeW(HKEY_CLASSES_ROOT, &clsid_path) };
    if hr != ERROR_SUCCESS {
        log::warn!("[tip_manager] RegDeleteTreeW(HKCR/{clsid_str}) => {hr:?}");
    }

    let ctf_tip_path = HSTRING::from(format!("SOFTWARE\\Microsoft\\CTF\\TIP\\{clsid_str}"));
    let hr = unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, &ctf_tip_path) };
    if hr != ERROR_SUCCESS {
        log::warn!("[tip_manager] RegDeleteTreeW(HKLM/CTF/TIP/{clsid_str}) => {hr:?}");
    }

    log::info!("[tip_manager] unregister_tip: CLSID={clsid_str}");
    Ok(())
}

fn set_reg_dword(
    root: HKEY,
    key_path: &HSTRING,
    value_name: PCWSTR,
    value: u32,
) -> Result<(), TipManagerError> {
    let mut sub_key = HKEY::default();
    let access = REG_SAM_FLAGS(KEY_SET_VALUE.0 | KEY_CREATE_SUB_KEY.0);
    let status = unsafe {
        RegCreateKeyExW(
            root,
            key_path,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut sub_key,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(TipManagerError::Registry(format!(
            "RegCreateKeyExW 失败: {status:?}"
        )));
    }
    let bytes = value.to_le_bytes();
    let status = unsafe { RegSetValueExW(sub_key, value_name, None, REG_DWORD, Some(&bytes)) };
    unsafe { let _ = RegCloseKey(sub_key); };
    if status != ERROR_SUCCESS {
        return Err(TipManagerError::Registry(format!(
            "RegSetValueExW(DWORD) 失败: {status:?}"
        )));
    }
    Ok(())
}

fn set_reg_sz(
    root: HKEY,
    key_path: &HSTRING,
    value_name: PCWSTR,
    value: &HSTRING,
) -> Result<(), TipManagerError> {
    let mut sub_key = HKEY::default();
    let access = REG_SAM_FLAGS(KEY_SET_VALUE.0 | KEY_CREATE_SUB_KEY.0);
    let status = unsafe {
        RegCreateKeyExW(
            root,
            key_path,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut sub_key,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(TipManagerError::Registry(format!(
            "RegCreateKeyExW 失败: {status:?}"
        )));
    }
    // 将 value 编码为 UTF-16 字节（含 null terminator）
    let mut value_wide: Vec<u16> = value.iter().copied().collect();
    value_wide.push(0);
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(
            value_wide.as_ptr() as *const u8,
            value_wide.len() * 2,
        )
    };
    let status = unsafe { RegSetValueExW(sub_key, value_name, None, REG_SZ, Some(bytes)) };
    unsafe { let _ = RegCloseKey(sub_key); };
    if status != ERROR_SUCCESS {
        return Err(TipManagerError::Registry(format!(
            "RegSetValueExW(SZ) 失败: {status:?}"
        )));
    }
    Ok(())
}
