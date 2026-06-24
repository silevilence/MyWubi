//! Windows 注册表的 TIP 注册与反注册实现。
//!
//! 在 `regsvr32 im_engine.dll` 触发后，操作系统的 `DllRegisterServer` 入口
//! 会调用本模块，把本 COM 类 (`CLSID_TEXT_SERVICE`) 注册为：
//!
//! * `HKEY_CLASSES_ROOT\CLSID\{CLSID}\` 下登记本类的 `InProcServer32`；
//! * `HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\CTF\TIP\{CLSID}\` 下登记语言
//!   配置（TIP 主注册表节点）。
//!
//! `DllUnregisterServer` 反转上述操作。本模块同时导出可被外部 `reg_script`
//! 调用的入口，便于脱离 regsvr32 流程进行注册（如 Velopack 安装 Hook 中）。

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CLASSES_ROOT,
    HKEY_LOCAL_MACHINE, KEY_CREATE_SUB_KEY, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
    REG_DWORD, REG_SAM_FLAGS, REG_SZ,
};

use crate::guids::{clsid_string, CLSID_TEXT_SERVICE, GUID_PROFILE, TEXT_SERVICE_NAME};

/// 成功返回的 WIN32_ERROR 0，等价于 `ERROR_SUCCESS`。
const ERROR_SUCCESS: WIN32_ERROR = WIN32_ERROR(0);

/// 我们的 DLL module handle（在 DllMain 中存储），用于查询 DLL 文件绝对路径。
static MODULE_HANDLE: parking_lot::Mutex<usize> = parking_lot::Mutex::new(0);

/// 由 DllMain 调用，缓存 `hInstance`，供后续注册表写入使用。
pub fn set_module_handle(handle: usize) {
    *MODULE_HANDLE.lock() = handle;
}

/// 获取缓存的 DLL module handle，供 `RegisterClassW` 等需要 HINSTANCE 的 API 使用。
pub fn module_handle() -> usize {
    *MODULE_HANDLE.lock()
}

/// 注册本 TIP。返回 `Ok(())` 仅当所有写键操作成功；失败返回 HRESULT 错误。
pub fn register_tip() -> windows::core::Result<()> {
    // 1. 获取本 DLL 绝对路径。
    let dll_path = current_dll_path()?;

    let clsid_str = clsid_string();
    let clsid_wide = HSTRING::from(&clsid_str);

    // 2. HKCR\CLSID\{CLSID}
    let clsid_path = HSTRING::from(format!("CLSID\\{clsid_str}"));
    set_reg_sz(HKEY_CLASSES_ROOT, &clsid_path, PCWSTR::null(), &HSTRING::from(TEXT_SERVICE_NAME))?;

    // 3. ... \InprocServer32 = <DLL 路径> + ThreadingModel = Apartment
    let inproc_path = HSTRING::from(format!("CLSID\\{clsid_str}\\InprocServer32"));
    set_reg_sz(HKEY_CLASSES_ROOT, &inproc_path, PCWSTR::null(), &dll_path)?;
    set_reg_sz(HKEY_CLASSES_ROOT, &inproc_path, w!("ThreadingModel"), &HSTRING::from("Apartment"))?;

    // 4. ... \ProgID = 友好的 Prog ID。
    let progid_path = HSTRING::from(format!("CLSID\\{clsid_str}\\ProgID"));
    set_reg_sz(HKEY_CLASSES_ROOT, &progid_path, PCWSTR::null(), &HSTRING::from("MyWubi.TextService.1"))?;

    // 4.5. HKCR\CLSID\{CLSID}\Implemented Categories\{CATID_TIP}
    //      声明本 COM 类是 TSF 文本服务——缺少此类别会导致键盘列表中灰显"仅桌面"
    let catid_tip = "{34745C63-B2F0-4784-8B67-5E12C8701A31}";
    let cat_path = HSTRING::from(format!("CLSID\\{clsid_str}\\Implemented Categories\\{catid_tip}"));
    set_reg_sz(HKEY_CLASSES_ROOT, &cat_path, PCWSTR::null(), &HSTRING::from(""))?;

    // 5. HKLM\SOFTWARE\Microsoft\CTF\TIP\{CLSID}
    let ctf_tip_path = format!("SOFTWARE\\Microsoft\\CTF\\TIP\\{clsid_str}");
    let ctf_tip_w = HSTRING::from(&ctf_tip_path);
    set_reg_sz(HKEY_LOCAL_MACHINE, &ctf_tip_w, PCWSTR::null(), &HSTRING::from(TEXT_SERVICE_NAME))?;

    // 6. LanguageProfile\{GUID_PROFILE}
    let profile_string = format!("{{{:?}}}", GUID_PROFILE);
    let lp_key_path = HSTRING::from(format!("{ctf_tip_path}\\LanguageProfile"));
    set_reg_sz(HKEY_LOCAL_MACHINE, &lp_key_path, PCWSTR::null(), &HSTRING::from(&profile_string))?;
    // 7. 完整 LanguageProfile —— 关联简体中文 (0x00000804)，Enable=1（允许用户添加）
    let lang_id = "0x00000804";
    let profile_path = HSTRING::from(format!(
        "{ctf_tip_path}\\LanguageProfile\\{lang_id}\\{profile_string}"
    ));
    set_reg_sz(HKEY_LOCAL_MACHINE, &profile_path, w!("Description"), &HSTRING::from(TEXT_SERVICE_NAME))?;
    set_reg_sz(HKEY_LOCAL_MACHINE, &profile_path, w!("IconFile"), &dll_path)?;
    set_reg_dword(HKEY_LOCAL_MACHINE, &profile_path, w!("IconIndex"), 0)?;
    set_reg_dword(HKEY_LOCAL_MACHINE, &profile_path, w!("Enable"), 1)?; // 1 = 允许从键盘列表添加

    // 7.5. Display Description — 在键盘列表中显示的友好名称
    set_reg_sz(
        HKEY_LOCAL_MACHINE,
        &ctf_tip_w,
        w!("Display Description"),
        &HSTRING::from(TEXT_SERVICE_NAME),
    )?;

    // 7.6. EnableCompatibleTsf —— 声明兼容现代 TSF。缺此键会被标记为"仅桌面"
    set_reg_dword(
        HKEY_LOCAL_MACHINE,
        &ctf_tip_w,
        w!("EnableCompatibleTsf"),
        1,
    )?;

    // 7.7. TIP Category —— 声明为键盘输入法类别
    let cat_tip = "{34745C63-B2F0-4784-8B67-5E12C8701A31}";
    let cat_keyboard = "{3640E571-E878-4FE7-B341-35D393003EAB}";
    let cat_tip_path = HSTRING::from(format!("{ctf_tip_path}\\Category\\Category{cat_tip}"));
    let cat_kb_path = HSTRING::from(format!("{ctf_tip_path}\\Category\\Category{cat_keyboard}"));
    set_reg_sz(HKEY_LOCAL_MACHINE, &cat_tip_path, PCWSTR::null(), &HSTRING::from(""))?;
    set_reg_sz(HKEY_LOCAL_MACHINE, &cat_kb_path, PCWSTR::null(), &HSTRING::from(""))?;

    // 8. 把 CLSID 写入 TIP 自身边节，用于系统识别 TIP COM 类。
    set_reg_sz(
        HKEY_LOCAL_MACHINE,
        &HSTRING::from(format!("{ctf_tip_path}\\CLSID")),
        PCWSTR::null(),
        &clsid_wide,
    )?;

    log::info!("[TSF] register_tip: CLSID={clsid_str} dll={}", dll_path);
    Ok(())
}

/// 反注册本 TIP。
pub fn unregister_tip() -> windows::core::Result<()> {
    let clsid_str = clsid_string();

    // 删除 HKCR\CLSID\{CLSID} 子树
    let clsid_path = HSTRING::from(format!("CLSID\\{clsid_str}"));
    let hr = unsafe { RegDeleteTreeW(HKEY_CLASSES_ROOT, &clsid_path) };
    if hr != ERROR_SUCCESS {
        log::warn!("[TSF] RegDeleteTreeW(HKCR/{clsid_str}) => {hr:?}");
    }

    // 删除 HKLM\SOFTWARE\Microsoft\CTF\TIP\{CLSID} 子树
    let ctf_tip_path = HSTRING::from(format!("SOFTWARE\\Microsoft\\CTF\\TIP\\{clsid_str}"));
    let hr = unsafe { RegDeleteTreeW(HKEY_LOCAL_MACHINE, &ctf_tip_path) };
    if hr != ERROR_SUCCESS {
        log::warn!("[TSF] RegDeleteTreeW(HKLM/CTF/TIP/{clsid_str}) => {hr:?}");
    }

    log::info!("[TSF] unregister_tip: CLSID={clsid_str}");
    Ok(())
}

/// 写一个 `REG_DWORD` 值。
fn set_reg_dword(
    root: HKEY,
    key_path: &HSTRING,
    value_name: PCWSTR,
    value: u32,
) -> windows::core::Result<()> {
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
        log::error!("[TSF] RegCreateKeyExW({key_path:?}) => {status:?}");
        return Err(windows::core::HRESULT(-1).into());
    }
    let bytes = value.to_le_bytes();
    let status = unsafe {
        RegSetValueExW(sub_key, value_name, None, REG_DWORD, Some(&bytes))
    };
    unsafe { let _ = RegCloseKey(sub_key); };
    if status != ERROR_SUCCESS {
        log::error!("[TSF] RegSetValueExW({key_path:?}, {value_name:?}) => {status:?}");
        return Err(windows::core::HRESULT(-1).into());
    }
    Ok(())
}

/// 写一个 `REG_SZ` 值。`value_name == PCWSTR::null()` 表示写默认值。
fn set_reg_sz(
    root: HKEY,
    key_path: &HSTRING,
    value_name: PCWSTR,
    value: &HSTRING,
) -> windows::core::Result<()> {
    let mut sub_key = HKEY::default();

    // 创建或打开子键，访问权限包含 set_value 和 create_sub_key。
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
        log::error!("[TSF] RegCreateKeyExW({key_path:?}) => {status:?}");
        return Err(windows::core::HRESULT(-1).into());
    }

    // utf-16 编码 + 末尾 NULL，作为 REG_SZ 写入。
    let mut data: Vec<u16> = value.iter().copied().collect();
    data.push(0);
    let bytes: Vec<u8> = data.iter().flat_map(|wv| wv.to_le_bytes()).collect();
    let status = unsafe {
        RegSetValueExW(sub_key, value_name, None, REG_SZ, Some(bytes.as_slice()))
    };
    unsafe { let _ = RegCloseKey(sub_key); };
    if status != ERROR_SUCCESS {
        log::error!(
            "[TSF] RegSetValueExW({key_path:?}, {value_name:?}) => {status:?}"
        );
        return Err(windows::core::HRESULT(-1).into());
    }
    Ok(())
}

/// 读取当前 DLL 文件绝对路径（作为 InprocServer32 注册值）。
fn current_dll_path() -> windows::core::Result<HSTRING> {
    let handle = *MODULE_HANDLE.lock();
    if handle == 0 {
        return Err(windows::core::HRESULT(-1).into());
    }
    let mut buffer = [0u16; 1024];
    let hmodule = windows::Win32::Foundation::HMODULE(handle as *mut _);
    let len = unsafe { GetModuleFileNameW(Some(hmodule), &mut buffer) };
    if len == 0 {
        return Err(windows::core::HRESULT(-1).into());
    }
    let utf16: Vec<u16> = buffer[..(len as usize)].iter().copied().collect();
    Ok(HSTRING::from_wide(&utf16))
}

/// 让 CLSID 常量在本模块被引用，避免 unused_imports。
#[allow(dead_code)]
fn _touch_constants() {
    let _ = CLSID_TEXT_SERVICE;
}