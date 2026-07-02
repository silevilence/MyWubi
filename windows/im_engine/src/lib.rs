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

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Once, OnceLock};

use arc_swap::ArcSwap;
use core_engine::{Config, Dictionary, StateMachine, UserDictionary};
use parking_lot::Mutex;

use crate::candidate_data::{CandidateData, ThemeSnapshot};
use windows::core::{ComObject, GUID, HRESULT};
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Foundation::CLASS_E_CLASSNOTAVAILABLE;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

pub mod candidate_data;
pub mod candidate_window;
pub mod factory;
pub mod guids;
pub mod file_log;
pub mod key_filter;
pub mod reload;
pub mod screen_geometry;
pub mod text_service;

/// 内部引擎单例，对应早期 ROADMAP 阶段“工作空间骨架”：保持 C-ABI 入口
/// `im_engine_init/_on_key/_destroy` 兼容的初始化路径。
struct Engine {
    runtime: Arc<ArcSwap<RuntimeSnapshot>>,
    #[allow(dead_code)]
    sm: Mutex<StateMachine>,
    candidate_data: Arc<ArcSwap<CandidateData>>,
}

impl Engine {
    fn new(runtime: Arc<ArcSwap<RuntimeSnapshot>>, sm: StateMachine, cd: Arc<ArcSwap<CandidateData>>) -> Self {
        Self { runtime, sm: Mutex::new(sm), candidate_data: cd }
    }

    pub fn candidate_data(&self) -> &Arc<ArcSwap<CandidateData>> {
        &self.candidate_data
    }

    pub fn runtime(&self) -> &Arc<ArcSwap<RuntimeSnapshot>> {
        &self.runtime
    }
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
static NEXT_RUNTIME_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(crate) struct RuntimeSnapshot {
    pub revision: u64,
    pub dict: Arc<Dictionary>,
    pub config: Config,
    pub config_path: PathBuf,
    pub system_table_path: PathBuf,
    pub user_table_path: PathBuf,
}

impl RuntimeSnapshot {
    fn initial_from_config(config_path: PathBuf, config: Config) -> Result<Self, String> {
        let system_table_path = core_engine::config_path::resolve_resource_path(
            &config_path,
            &config.dictionary.system_table,
        );
        let user_table_path = core_engine::config_path::resolve_resource_path(
            &config_path,
            &config.dictionary.user_table,
        );
        let dict = load_dictionary(&system_table_path, &user_table_path, &config)?;
        Ok(Self {
            revision: NEXT_RUNTIME_REVISION.fetch_add(1, Ordering::Relaxed),
            dict,
            config,
            config_path,
            system_table_path,
            user_table_path,
        })
    }
}

fn load_dictionary(
    system_table_path: &std::path::Path,
    user_table_path: &std::path::Path,
    config: &Config,
) -> Result<Arc<Dictionary>, String> {
    let system = Dictionary::load(system_table_path)
        .map_err(|error| format!("加载码表失败 ({}): {error}", system_table_path.display()))?;
    if !config.dictionary.enable_user_dict {
        return Ok(system);
    }

    let user = UserDictionary::load(user_table_path)
        .map_err(|error| format!("加载用户词库失败 ({}): {error}", user_table_path.display()))?;
    let user_keys: HashSet<(&str, &str)> = user
        .entries()
        .iter()
        .map(|entry| (entry.code.as_str(), entry.word.as_str()))
        .collect();
    let mut entries = user.entries().to_vec();
    entries.extend(
        system
            .entries()
            .iter()
            .filter(|entry| !user_keys.contains(&(entry.code.as_str(), entry.word.as_str())))
            .cloned(),
    );
    system
        .rebuild_with_entries(entries, Default::default())
        .map_err(|error| format!("合并用户词库失败: {error}"))
}

pub(crate) fn load_runtime_snapshot() -> Result<RuntimeSnapshot, String> {
    let exe_dir = dll_directory()
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位 DLL 目录".to_string())?;
    let app_dir = dirs::config_dir()
        .ok_or_else(|| "无法获取 AppData 路径".to_string())?
        .join("MyWubi");
    let resolved = core_engine::config_path::resolve_config_path_from(&exe_dir, &app_dir)
        .map_err(|e| format!("定位配置路径失败: {e}"))?;
    let config = Config::load(&resolved.path)
        .map_err(|e| format!("加载配置失败 ({}): {e}", resolved.path.display()))?;
    RuntimeSnapshot::initial_from_config(resolved.path, config)
}

fn load_initial_runtime_snapshot() -> RuntimeSnapshot {
    match load_runtime_snapshot() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            log::error!("{err}");
            let exe_dir = dll_directory().map(PathBuf::from);
            let app_dir = dirs::config_dir().map(|p| p.join("MyWubi"));
            let resolved_path = exe_dir
                .as_ref()
                .zip(app_dir.as_ref())
                .and_then(|(exe_dir, app_dir)| {
                    core_engine::config_path::resolve_config_path_from(exe_dir, app_dir)
                        .ok()
                        .map(|r| r.path)
                })
                .unwrap_or_else(|| PathBuf::from("config.toml"));

            let config = Config::load(&resolved_path).unwrap_or_default();
            let system_table_path = core_engine::config_path::resolve_resource_path(
                &resolved_path,
                &config.dictionary.system_table,
            );
            let user_table_path = core_engine::config_path::resolve_resource_path(
                &resolved_path,
                &config.dictionary.user_table,
            );
            let dict = load_dictionary(&system_table_path, &user_table_path, &config)
                .unwrap_or_else(|dict_err| {
                log::error!("加载码表失败 ({}): {dict_err}", system_table_path.display());
                Dictionary::from_entries(Vec::new(), None, Default::default())
                    .expect("empty dictionary should construct")
            });
            RuntimeSnapshot {
                revision: NEXT_RUNTIME_REVISION.fetch_add(1, Ordering::Relaxed),
                dict,
                config,
                config_path: resolved_path,
                system_table_path,
                user_table_path,
            }
        }
    }
}

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

    let runtime_snapshot = load_initial_runtime_snapshot();
    let sm = StateMachine::with_options(
        Arc::clone(&runtime_snapshot.dict),
        runtime_snapshot.config.basic.candidate_count as usize,
        runtime_snapshot.config.basic.auto_commit_unique,
    );
    let theme = ThemeSnapshot::from_config(&runtime_snapshot.config);
    let cd = Arc::new(ArcSwap::from_pointee(CandidateData::hidden(theme)));
    let runtime = Arc::new(ArcSwap::from_pointee(runtime_snapshot));
    reload::spawn(Arc::clone(&runtime));
    let _ = ENGINE.set(Engine::new(runtime, sm, cd));
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
#[allow(dead_code)]
pub(crate) fn dll_directory() -> Option<String> {
    let dll_path = get_this_dll_path().ok()?;
    let parent = std::path::Path::new(&dll_path).parent()?;
    Some(parent.to_str()?.to_string() + "\\")
}

/// 获取当前 DLL 文件绝对路径。
fn get_this_dll_path() -> Result<String, windows::core::Error> {
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
    let mut buf = vec![0u16; 260];
    let handle = module_handle();
    let hmod = if handle != 0 {
        Some(HMODULE(handle as *mut _))
    } else {
        None
    };
    let len = unsafe { GetModuleFileNameW(hmod, &mut buf) as usize };
    if len == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(test)]
mod runtime_tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use core_engine::Entry;

    use super::*;

    #[test]
    fn user_dictionary_is_merged_without_duplicate_candidates() {
        let root = std::env::temp_dir().join(format!(
            "mywubi-runtime-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let system_path = root.join("system.dict");
        let user_path = root.join("user.dict");
        std::fs::write(
            &system_path,
            "---\nwildcard_key: z\ncharset: abcdefghijklmnopqrstuvwxyz\n---\nabcd\t系统词\t1\nabcd\t覆盖词\t1\n",
        )
        .unwrap();
        let mut user = UserDictionary::load(&user_path).unwrap();
        user.add(Entry {
            code: "abcd".into(),
            word: "覆盖词".into(),
            weight: 100,
        })
        .unwrap();
        let mut config = Config::default();
        config.dictionary.enable_user_dict = true;

        let dictionary = load_dictionary(&system_path, &user_path, &config).unwrap();

        assert_eq!(
            dictionary
                .exact("abcd")
                .iter()
                .map(|entry| entry.word.as_str())
                .collect::<Vec<_>>(),
            ["覆盖词", "系统词"]
        );
        assert_eq!(dictionary.table_config().wildcard_key, Some('z'));
        let _ = std::fs::remove_dir_all(root);
    }
}
