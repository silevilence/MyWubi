//! # im_engine
//!
//! Windows TSF 输入法本体 DLL。本模块对应 ROADMAP“TSF 接口对接与 COM 注册”
//! 子任务，导出标准 COM 服务器入口：
//!
//! * `DllMain` —— 初始化日志、记录模块 handle；
//! * `DllGetClassObject(rclsid, riid, ppv)` —— 系统请求工厂；
//! * `DllCanUnloadNow()` —— 系统查询是否可卸载；
//! * `DllRegisterServer()` / `DllUnregisterServer()` —— 与 `regsvr32` 配合。
//!
//! 所有的 COM 接口实现细节见 [`text_service`] 模块。

#![cfg_attr(not(windows), allow(unused))]

use arc_swap::ArcSwap;
use core_engine::{Config, Dictionary, StateMachine};
use parking_lot::Mutex;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::Once;
use std::sync::OnceLock;

use crate::candidate_data::CandidateData;
use windows::core::{ComObject, GUID, HRESULT};
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Foundation::CLASS_E_CLASSNOTAVAILABLE;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

pub mod candidate_data;
pub mod candidate_renderer;
pub mod candidate_window;
pub mod factory;
pub mod guids;
pub mod key_filter;
pub mod file_log;
pub mod screen_geometry;
pub mod text_service;

slint::include_modules!();

/// 内部引擎单例，对应早期 ROADMAP 阶段“工作空间骨架”：保持 C-ABI 入口
/// `im_engine_init/_on_key/_destroy` 兼容的初始化路径。
struct Engine {
    dict: Arc<Dictionary>,
    #[allow(dead_code)]
    sm: Mutex<StateMachine>,
    candidate_data: Arc<ArcSwap<CandidateData>>,
}

impl Engine {
    fn new(dict: Arc<Dictionary>, sm: StateMachine, cd: Arc<ArcSwap<CandidateData>>) -> Self {
        Self { dict, sm: Mutex::new(sm), candidate_data: cd }
    }

    pub fn candidate_data(&self) -> &Arc<ArcSwap<CandidateData>> {
        &self.candidate_data
    }

    pub fn dict(&self) -> &Arc<Dictionary> {
        &self.dict
    }
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

/// 初始化引擎：加载配置与码表，构建状态机。返回 0 表示成功。
///
/// 此函数对应 JNI/TSF 桥接层的 `init` 入口（保留给后台进程或 Velopack
/// 安装 Hook 主动初始化时调用）。
#[no_mangle]
pub extern "C" fn im_engine_init() -> i32 {
    // 惰性初始化文件日志
    file_log::init();

    // 惰性设置 panic hook（首次调用时），避免在 DllMain loader lock 下分配内存
    static PANIC_HOOK_SET: Once = Once::new();
    PANIC_HOOK_SET.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            let loc = info.location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default();
            log::error!("[PANIC] {msg} at {loc}");
        }));
    });

    if ENGINE.get().is_some() {
        return 0;
    }

    // 所有路径均基于 DLL 所在目录解析，而非当前工作目录（ctfmon.exe 的 CWD）。
    let dll_dir = dll_directory().unwrap_or_default();

    let cfg_path = format!("{}config.toml", dll_dir);
    let cfg = match Config::load(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            log::error!("加载配置失败 ({cfg_path}): {e}, 使用默认配置");
            Config::default()
        }
    };

    // 码表路径同样基于 DLL 目录解析
    let table_path = if std::path::Path::new(&cfg.dictionary.system_table).is_relative() {
        format!("{}{}", dll_dir, cfg.dictionary.system_table.display())
    } else {
        cfg.dictionary.system_table.display().to_string()
    };
    let dict = match Dictionary::load(&table_path) {
        Ok(d) => d,
        Err(e) => {
            log::error!("加载码表失败 ({table_path}): {e}");
            match Dictionary::from_entries(Vec::new(), None, Default::default()) {
                Ok(d) => d,
                Err(e2) => {
                    log::error!("创建空码表失败: {e2}");
                    return -1;
                }
            }
        }
    };
    let sm = StateMachine::with_options(
        Arc::clone(&dict),
        cfg.basic.candidate_count as usize,
        cfg.basic.auto_commit_unique,
    );
    let cd = Arc::new(ArcSwap::from_pointee(CandidateData::default()));
    let _ = ENGINE.set(Engine::new(dict, sm, cd));
    0
}

/// 处理一个按键的占位实现（保留 C-ABI），返回上屏文本长度（0 表示无上屏）。
#[no_mangle]
pub extern "C" fn im_engine_on_key(_code: i32) -> i32 {
    if ENGINE.get().is_none() {
        return -1;
    }
    0
}

/// 释放引擎资源（由 OS 回收，OnceLock 不可显式 take）。
#[no_mangle]
pub extern "C" fn im_engine_destroy() {}

// ── COM 服务器导出 ────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn DllMain(
    h_instance: HMODULE,
    reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> bool {
    if reason == DLL_PROCESS_ATTACH {
        set_module_handle(h_instance.0 as usize);
        // panic hook 移到 im_engine_init 中惰性设置，避免在 loader lock 下分配内存
    }
    true
}

/// 系统在 `CoGetClassObject` 时调用本函数请求 COM 类对象（即 ClassFactory）。
#[no_mangle]
pub extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut core::ffi::c_void,
) -> HRESULT {
    // 惰性初始化文件日志（安全：不在 loader lock 下）
    file_log::init();

    let result = catch_unwind(AssertUnwindSafe(|| {
        dll_get_class_object_impl(rclsid, riid, ppv)
    }));
    match result {
        Ok(hr) => hr,
        Err(e) => {
            log::error!("[TSF] DllGetClassObject panic: {:?}", e);
            CLASS_E_CLASSNOTAVAILABLE
        }
    }
}

fn dll_get_class_object_impl(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut core::ffi::c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    unsafe { *ppv = std::ptr::null_mut(); }

    let clsid = unsafe { &*rclsid };
    if *clsid != guids::CLSID_TEXT_SERVICE {
        log::warn!("[TSF] DllGetClassObject: 未知 CLSID {clsid:?}");
        return CLASS_E_CLASSNOTAVAILABLE;
    }

    // 每次调用创建一个新的 ClassFactory（工厂状态无副作用），调用方持有时
    // 活可，释放后 ComObject 被自动回收。工厂中创建的 TextService 通过同一
    // ComObject 管理。
    let factory_obj = ComObject::new(factory::TextServiceFactory);
    let unk: windows::core::IUnknown = factory_obj.to_interface();
    let iid = unsafe { &*riid };
    let mut raw: *mut core::ffi::c_void = std::ptr::null_mut();
    let hr = unsafe { Interface::query(&unk, iid, &mut raw) };
    if hr.is_ok() && !raw.is_null() {
        unsafe { *ppv = raw; }
        HRESULT(0)
    } else {
        log::error!("[TSF] DllGetClassObject: QueryInterface({iid:?}) => {hr:?}");
        CLASS_E_CLASSNOTAVAILABLE
    }
}

/// 是否允许从进程卸载 DLL。若 `FACTORY` 与活动文本服务已无引用，则可卸载。
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    // 始终返回 S_FALSE：本 TIP 通常常驻 ctfmon，且工厂自身生命周期由 OnceLock
    // 持有，不能释放。
    // TODO(silev): 使用 windows::Win32::Foundation::S_FALSE 常量替代魔法数字
    HRESULT(1)
}

/// `regsvr32 im_engine.dll` 调用，写入 CLSID / TIP 注册表节点。
#[no_mangle]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    let dll_path = match get_this_dll_path() {
        Ok(p) => p,
        Err(_) => return windows::core::HRESULT(-1),
    };
    match ::tip_manager::install(&dll_path) {
        Ok(()) => windows::core::HRESULT(0),
        Err(e) => {
            log::error!("DllRegisterServer 失败: {e}");
            windows::core::HRESULT(-1)
        }
    }
}

/// `regsvr32 /u im_engine.dll` 调用，从注册表移除 CLSID / TIP 节点。
#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    match ::tip_manager::uninstall() {
        Ok(()) => windows::core::HRESULT(0),
        Err(e) => {
            log::error!("DllUnregisterServer 失败: {e}");
            windows::core::HRESULT(-1)
        }
    }
}

// ── 模块句柄管理 ────────────────────────────────────────────────

/// 我们的 DLL module handle（在 DllMain 中存储），供候选窗口创建等需要 HINSTANCE 的 API 使用。
static MODULE_HANDLE: parking_lot::Mutex<usize> = parking_lot::Mutex::new(0);

/// 由 DllMain 调用，缓存 `hInstance`。
fn set_module_handle(handle: usize) {
    *MODULE_HANDLE.lock() = handle;
}

/// 获取缓存的 DLL module handle。
pub(crate) fn module_handle() -> usize {
    *MODULE_HANDLE.lock()
}

/// 获取 DLL 所在目录（含尾部分隔符）。
pub(crate) fn dll_directory() -> Option<String> {
    let dll_path = get_this_dll_path().ok()?;
    let parent = std::path::Path::new(&dll_path).parent()?;
    Some(parent.to_str()?.to_string() + "\\")
}

/// 获取当前 DLL 文件绝对路径。
fn get_this_dll_path() -> Result<String, windows::core::Error> {
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
    let mut buf = vec![0u16; 260];
    let len = unsafe { GetModuleFileNameW(None, &mut buf) as usize };
    if len == 0 {
        return Err(windows::core::Error::from_win32());
    }
    Ok(String::from_utf16_lossy(&buf[..len]))
}