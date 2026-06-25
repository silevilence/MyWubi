# TIP 管理器与设置集成 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新建 `windows/tip_manager` crate 封装 TIP 全生命周期管理，在 settings 新增「输入法管理」面板，迁移 im_engine 注册逻辑。

**Architecture:** `tip_manager` 是 Windows 专用纯库 crate，封装注册表写入 + `ITfInputProcessorProfileMgr` COM 调用。`im_engine` 和 `settings` 均依赖它。`TipProfileManager` trait 实现 COM 依赖注入以支持单元测试。

**Tech Stack:** Rust 2021 · windows-rs (Registry + COM) · egui/eframe · winres (manifest)

**关联 Spec:** `docs/superpowers/specs/2026-06-25-tip-manager-design.md`

---

## 文件结构总览

```
windows/tip_manager/            # ← 新建 crate
├── Cargo.toml
└── src/
    ├── lib.rs                  # 公开 API + TipStatus 枚举
    ├── guids.rs                # 从 im_engine 迁移
    ├── registrar.rs            # 从 im_engine 迁移 + 重构
    ├── profile.rs              # ITfInputProcessorProfileMgr COM 封装
    ├── detect.rs               # 状态检测
    └── error.rs                # TipManagerError

windows/im_engine/
├── Cargo.toml                  # 修改：新增 tip_manager 依赖
└── src/
    ├── lib.rs                  # 修改：DllRegisterServer/UnregisterServer 改为委托 tip_manager
    └── registrar.rs            # 删除

windows/settings/
├── Cargo.toml                  # 修改：新增 tip_manager、winres 依赖
├── build.rs                    # 新建：嵌入 requireAdministrator manifest
└── src/
    ├── main.rs                 # 修改：启动时管理员检查
    ├── app.rs                  # 修改：新增 tip_manager 面板入口
    ├── state.rs                # 修改：AppState 新增 tip_status 字段
    └── panels/
        ├── mod.rs              # 修改：新增 Panel::TipManager 变体
        └── tip_manager.rs      # 新建：输入法管理面板 UI
```

---

### Task 1: 创建 `windows/tip_manager` crate 骨架

**Files:**
- Create: `windows/tip_manager/Cargo.toml`
- Create: `windows/tip_manager/src/lib.rs`
- Create: `windows/tip_manager/src/error.rs`
- Modify: `Cargo.toml`（workspace 根）

- [ ] **Step 1: 编写 Cargo.toml**

```toml
[package]
name = "tip_manager"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
log.workspace = true
thiserror.workspace = true

[target.'cfg(windows)'.dependencies]
windows = { workspace = true, features = [
    "Win32_Foundation",
    "Win32_System_Com",
    "Win32_System_Registry",
    "Win32_System_LibraryLoader",
    "Win32_UI_TextServices",
] }
windows-core = "0.61"
```

- [ ] **Step 2: 编写 error.rs**

```rust
//! TIP 管理器错误类型。

use std::path::PathBuf;

/// TIP 管理操作可能失败的所有方式。
#[derive(Debug, thiserror::Error)]
pub enum TipManagerError {
    /// 注册表操作失败。
    #[error("注册表操作失败: {0}")]
    Registry(String),

    /// COM 初始化或调用失败。
    #[error("COM 调用失败: {0}")]
    Com(String),

    /// 找不到 im_engine.dll。
    #[error("找不到 DLL: {0}")]
    DllNotFound(PathBuf),

    /// 权限不足（非管理员运行）。
    #[error("需要管理员权限")]
    AccessDenied,

    /// TIP 状态不一致（例如注册表有但 COM 找不到）。
    #[error("TIP 状态不一致: {0}")]
    InconsistentState(String),
}
```

- [ ] **Step 3: 编写 lib.rs 骨架（含 TipStatus 枚举 + 公开 API 签名）**

```rust
//! Windows TIP（Text Input Processor）管理器。
//!
//! 封装 TIP 全生命周期管理：注册表写入 + `ITfInputProcessorProfileMgr` COM 调用。

pub mod detect;
pub mod error;
pub mod guids;
pub mod profile;
pub mod registrar;

pub use error::TipManagerError;

/// TIP 当前安装与启用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipStatus {
    /// 系统中未找到本 TIP 的注册痕迹。
    NotInstalled,
    /// TIP 已注册但当前被禁用。
    InstalledDisabled,
    /// TIP 已注册且已启用。
    InstalledEnabled,
    /// 检测到异常状态（注册表残缺、COM 不可用等）。
    Unknown,
}

/// 安装 TIP：注册表写入 + COM RegisterProfile + EnableProfile。
///
/// `dll_path` 为 `im_engine.dll` 的绝对路径。
pub fn install(dll_path: &str) -> Result<(), TipManagerError> {
    todo!("Task 6")
}

/// 卸载 TIP：COM DisableProfile + UnregisterProfile + 注册表清理。
pub fn uninstall() -> Result<(), TipManagerError> {
    todo!("Task 6")
}

/// 启用已安装但禁用中的 TIP。
pub fn enable() -> Result<(), TipManagerError> {
    todo!("Task 6")
}

/// 禁用已安装且启用中的 TIP。
pub fn disable() -> Result<(), TipManagerError> {
    todo!("Task 6")
}

/// 检测当前 TIP 的安装与启用状态。
pub fn detect_status() -> TipStatus {
    todo!("Task 5")
}
```

- [ ] **Step 4: 在 workspace 根 Cargo.toml 注册成员**

编辑 `Cargo.toml` 的 `[workspace]` 段，在 `members` 数组中新增：

```toml
"windows/tip_manager",
```

位置：紧跟在 `"core_engine"` 之后或 `"windows/im_engine"` 之前均可。

- [ ] **Step 5: 编译验证**

Run: `cargo check -p tip_manager`
Expected: 编译通过（有 4 个 `todo!()` 警告是正常的）

- [ ] **Step 6: 提交**

```bash
git add windows/tip_manager/ Cargo.toml
git commit -m "🔧 chore(tip_manager): 创建 tip_manager crate 骨架

- 新建 windows/tip_manager crate，含 TipStatus 枚举与公开 API 签名
- 定义 TipManagerError 错误类型
- 注册为 workspace member"
```

---

### Task 2: 迁移 `guids.rs` 到 `tip_manager`

**Files:**
- Create: `windows/tip_manager/src/guids.rs`
- Modify: `windows/im_engine/src/guids.rs`（改为 re-export）
- Modify: `windows/im_engine/src/factory.rs`（更新 use 路径）
- Modify: `windows/im_engine/src/lib.rs`（更新 use 路径）

- [ ] **Step 1: 将 `im_engine/src/guids.rs` 内容复制到 `tip_manager/src/guids.rs`**

从 `windows/im_engine/src/guids.rs` 读取全部内容，写入 `windows/tip_manager/src/guids.rs`。

内容已存在，直接用 `im_engine/src/guids.rs` 当前版本（包含 `CLSID_TEXT_SERVICE`、`TEXT_SERVICE_NAME`、`GUID_PROFILE` 等所有常量及辅助函数）。

- [ ] **Step 2: 修改 `im_engine/src/guids.rs` 为 re-export**

将 `windows/im_engine/src/guids.rs` 内容替换为：

```rust
//! 本输入法 TIP（Text Input Processor）跨进程复用的 COM 标识符。
//!
//! 常量定义已迁移至 `tip_manager::guids`，本文件仅做 re-export 以保持
//! im_engine 内部 `crate::guids` 引用不中断。

pub use tip_manager::guids::*;
```

- [ ] **Step 3: 添加 `tip_manager` 依赖到 im_engine**

编辑 `windows/im_engine/Cargo.toml`，在 `[dependencies]` 段末尾新增：

```toml
tip_manager = { path = "../tip_manager" }
```

- [ ] **Step 4: 编译验证**

Run: `cargo check -p im_engine`
Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add windows/tip_manager/src/guids.rs windows/im_engine/src/guids.rs windows/im_engine/Cargo.toml
git commit -m "♻️ refactor(tip_manager): 迁移 guids.rs 到 tip_manager

- guids.rs 常量定义移至 tip_manager crate
- im_engine 改为 re-export，内部引用不中断"
```

---

### Task 3: 实现 `tip_manager::registrar`（注册表写入）

**Files:**
- Create: `windows/tip_manager/src/registrar.rs`

- [ ] **Step 1: 编写 registrar.rs**

基于 `windows/im_engine/src/registrar.rs` 当前逻辑，重构为 `tip_manager` 风格。关键差异：
- 不再持有 `MODULE_HANDLE` 静态变量——`dll_path` 由调用方传入
- 移除 `set_module_handle`/`module_handle` 函数
- 保留 `set_reg_sz`、`set_reg_dword` 辅助函数
- API：`register_tip(dll_path: &str) -> Result<(), TipManagerError>` 和 `unregister_tip() -> Result<(), TipManagerError>`

```rust
//! Windows 注册表的 TIP 注册与反注册实现。

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CLASSES_ROOT,
    HKEY_LOCAL_MACHINE, KEY_CREATE_SUB_KEY, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
    REG_DWORD, REG_SAM_FLAGS, REG_SZ,
};

use crate::error::TipManagerError;
use crate::guids::{clsid_string, CLSID_TEXT_SERVICE, GUID_PROFILE, TEXT_SERVICE_NAME};

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
    let value_wide: Vec<u16> = value.as_wide().iter().chain(std::iter::once(&0)).copied().collect();
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
```

- [ ] **Step 2: 编译验证**

Run: `cargo check -p tip_manager`
Expected: 编译通过（`lib.rs` 中 4 个 `todo!()` 仍然正常）

- [ ] **Step 3: 提交**

```bash
git add windows/tip_manager/src/registrar.rs
git commit -m "✨ feat(tip_manager): 实现注册表写入与删除逻辑

- register_tip() / unregister_tip() 从 im_engine 迁移
- dll_path 改为参数传入，不依赖全局 MODULE_HANDLE
- 错误类型改为 TipManagerError"
```

---

### Task 4: 实现 `tip_manager::profile`（ITfInputProcessorProfileMgr COM 封装）

**Files:**
- Create: `windows/tip_manager/src/profile.rs`

- [ ] **Step 1: 编写 profile.rs**

```rust
//! ITfInputProcessorProfileMgr COM 接口封装。
//!
//! 提供 `TipProfileManager` trait 及其 COM 实现，用于 TIP Profile 的
//! 注册/反注册/启用/禁用操作。

use windows::core::{ComObject, GUID, HRESULT, Ref, RefMut, Implement};
use windows::Win32::System::Com::{
    CoCreateInstance, CLSCTX_INPROC_SERVER,
};
use windows::Win32::UI::TextServices::{
    ITfInputProcessorProfileMgr, CLSID_TF_InputProcessorProfileMgr,
};

use crate::error::TipManagerError;
use crate::guids::{CLSID_TEXT_SERVICE, GUID_PROFILE};

/// 抽象 TIP Profile 管理接口，方便测试 mock。
pub trait TipProfileManager {
    fn is_enabled(&self) -> Result<bool, TipManagerError>;
    fn enable(&self) -> Result<(), TipManagerError>;
    fn disable(&self) -> Result<(), TipManagerError>;
    fn register_profile(&self) -> Result<(), TipManagerError>;
    fn unregister_profile(&self) -> Result<(), TipManagerError>;
}

/// 真实 COM 实现：通过 `ITfInputProcessorProfileMgr` 操作系统 TIP Profile。
pub struct ComProfileManager {
    mgr: ITfInputProcessorProfileMgr,
    /// 简中语言 ID。
    lang_id: u32,
}

impl ComProfileManager {
    /// 创建并初始化 COM 接口实例。
    pub fn new() -> Result<Self, TipManagerError> {
        let mgr: ITfInputProcessorProfileMgr = unsafe {
            CoCreateInstance(&CLSID_TF_InputProcessorProfileMgr, None, CLSCTX_INPROC_SERVER)
        }.map_err(|e| TipManagerError::Com(format!("CoCreateInstance(ITfInputProcessorProfileMgr) 失败: {e}")))?;

        Ok(Self {
            mgr,
            lang_id: 0x0804, // 简体中文 LANGID (low word of 0x00000804)
        })
    }
}

impl TipProfileManager for ComProfileManager {
    fn is_enabled(&self) -> Result<bool, TipManagerError> {
        let mut enabled = windows::Win32::Foundation::BOOL::default();
        unsafe {
            self.mgr.IsEnabledProfile(
                &CLSID_TEXT_SERVICE,
                &GUID_PROFILE,
                self.lang_id,
                &mut enabled,
            )
        }.map_err(|e| TipManagerError::Com(format!("IsEnabledProfile 失败: {e}")))?;
        Ok(enabled.as_bool())
    }

    fn enable(&self) -> Result<(), TipManagerError> {
        unsafe {
            self.mgr.EnableProfile(
                &CLSID_TEXT_SERVICE,
                &GUID_PROFILE,
                self.lang_id,
                true,
            )
        }.map_err(|e| TipManagerError::Com(format!("EnableProfile 失败: {e}")))
    }

    fn disable(&self) -> Result<(), TipManagerError> {
        unsafe {
            self.mgr.EnableProfile(
                &CLSID_TEXT_SERVICE,
                &GUID_PROFILE,
                self.lang_id,
                false,
            )
        }.map_err(|e| TipManagerError::Com(format!("DisableProfile 失败: {e}")))
    }

    fn register_profile(&self) -> Result<(), TipManagerError> {
        unsafe {
            self.mgr.RegisterProfile(
                &CLSID_TEXT_SERVICE,
                self.lang_id,
                &GUID_PROFILE,
                &windows::core::HSTRING::from(crate::guids::TEXT_SERVICE_NAME),
                None, // icon file — 已在注册表中指定
                None, // icon index
                None, // HKL
                0,
                true, // 允许用户通过控制面板添加
            )
        }.map_err(|e| TipManagerError::Com(format!("RegisterProfile 失败: {e}")))
    }

    fn unregister_profile(&self) -> Result<(), TipManagerError> {
        unsafe {
            self.mgr.UnregisterProfile(
                &CLSID_TEXT_SERVICE,
                self.lang_id,
                &GUID_PROFILE,
                windows::Win32::Foundation::TRUE,
            )
        }.map_err(|e| TipManagerError::Com(format!("UnregisterProfile 失败: {e}")))
    }
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
            if self.fail_next.get() { return Err(TipManagerError::Com("mock failure".into())); }
            self.enabled.set(true);
            Ok(())
        }

        fn disable(&self) -> Result<(), TipManagerError> {
            if self.fail_next.get() { return Err(TipManagerError::Com("mock failure".into())); }
            self.enabled.set(false);
            Ok(())
        }

        fn register_profile(&self) -> Result<(), TipManagerError> {
            if self.fail_next.get() { return Err(TipManagerError::Com("mock failure".into())); }
            self.registered.set(true);
            self.enabled.set(true);
            Ok(())
        }

        fn unregister_profile(&self) -> Result<(), TipManagerError> {
            if self.fail_next.get() { return Err(TipManagerError::Com("mock failure".into())); }
            self.registered.set(false);
            Ok(())
        }
    }
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check -p tip_manager`
Expected: 编译通过

- [ ] **Step 3: 提交**

```bash
git add windows/tip_manager/src/profile.rs
git commit -m "✨ feat(tip_manager): 实现 ITfInputProcessorProfileMgr COM 封装

- TipProfileManager trait + ComProfileManager COM 实现
- MockProfileManager 用于测试
- 支持 enable/disable/register/unregister Profile 操作"
```

---

### Task 5: 实现 `tip_manager::detect`（状态检测）

**Files:**
- Create: `windows/tip_manager/src/detect.rs`
- Modify: `windows/tip_manager/src/lib.rs`（填充 detect_status 函数体）

- [ ] **Step 1: 编写 detect.rs（内部检测逻辑）**

```rust
//! TIP 状态检测。
//!
//! 通过注册表查询 + COM `IsEnabledProfile` 判断当前 TIP 安装与启用状态。

use windows::core::HSTRING;
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Registry::{
    RegOpenKeyExW, RegCloseKey, HKEY, HKEY_CLASSES_ROOT, KEY_READ,
};

use crate::error::TipManagerError;
use crate::guids::clsid_string;
use crate::profile::TipProfileManager;
use crate::TipStatus;

const ERROR_SUCCESS: WIN32_ERROR = WIN32_ERROR(0);
const ERROR_FILE_NOT_FOUND: WIN32_ERROR = WIN32_ERROR(2);

/// 检查注册表中 TIP CLSID 键是否存在。
fn is_registry_present() -> bool {
    let clsid_str = clsid_string();
    let inproc_path = HSTRING::from(format!("CLSID\\{clsid_str}\\InprocServer32"));
    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CLASSES_ROOT,
            &inproc_path,
            None,
            KEY_READ,
            &mut key,
        )
    };
    if status == ERROR_SUCCESS {
        unsafe { let _ = RegCloseKey(key); };
        true
    } else {
        false
    }
}

/// 检测 TIP 状态。
///
/// `profile_mgr` 参数允许注入 mock 用于测试。
/// 若传 `None`，内部创建真实 `ComProfileManager`。
pub fn detect_status_impl(
    profile_mgr: Option<&dyn TipProfileManager>,
) -> TipStatus {
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
```

- [ ] **Step 2: 更新 lib.rs 中 detect_status() 实现**

将 `lib.rs` 中 `detect_status()` 的 `todo!()` 替换为：

```rust
/// 检测当前 TIP 的安装与启用状态。
pub fn detect_status() -> TipStatus {
    detect::detect_status_impl(None)
}
```

同时在 `lib.rs` 顶部确保有 `use crate::detect;`（或直接在函数体内用完整路径）。

- [ ] **Step 3: 编译验证**

Run: `cargo check -p tip_manager`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add windows/tip_manager/src/detect.rs windows/tip_manager/src/lib.rs
git commit -m "✨ feat(tip_manager): 实现 TIP 状态检测

- 查询注册表 CLSID 键判断是否已安装
- 通过 COM IsEnabledProfile 判断启用/禁用
- detect_status_impl 支持依赖注入用于测试"
```

---

### Task 6: 实现 `tip_manager` 公开 API（install/uninstall/enable/disable）

**Files:**
- Modify: `windows/tip_manager/src/lib.rs`（填充 4 个函数体）

- [ ] **Step 1: 更新 lib.rs 中的 install 函数**

```rust
/// 安装 TIP：注册表写入 + COM RegisterProfile + EnableProfile。
///
/// `dll_path` 为 `im_engine.dll` 的绝对路径。
pub fn install(dll_path: &str) -> Result<(), TipManagerError> {
    log::info!("[tip_manager] 开始安装 TIP，dll_path={dll_path}");

    // 1. 注册表写入
    registrar::register_tip(dll_path)?;

    // 2. COM 注册与启用 Profile
    let mgr = profile::ComProfileManager::new()?;
    mgr.register_profile()?;
    mgr.enable()?;

    log::info!("[tip_manager] TIP 安装完成");
    Ok(())
}

/// 卸载 TIP：COM DisableProfile + UnregisterProfile + 注册表清理。
pub fn uninstall() -> Result<(), TipManagerError> {
    log::info!("[tip_manager] 开始卸载 TIP");

    // 1. COM 禁用与反注册
    if let Ok(mgr) = profile::ComProfileManager::new() {
        if let Ok(true) = mgr.is_enabled() {
            let _ = mgr.disable();
        }
        let _ = mgr.unregister_profile();
    } else {
        log::warn!("[tip_manager] COM 不可用，跳过 Profile 清理，仅清理注册表");
    }

    // 2. 注册表清理
    registrar::unregister_tip()?;

    log::info!("[tip_manager] TIP 卸载完成");
    Ok(())
}

/// 启用已安装但禁用中的 TIP。
pub fn enable() -> Result<(), TipManagerError> {
    let mgr = profile::ComProfileManager::new()?;
    mgr.enable()?;
    log::info!("[tip_manager] TIP 已启用");
    Ok(())
}

/// 禁用已安装且启用中的 TIP。
pub fn disable() -> Result<(), TipManagerError> {
    let mgr = profile::ComProfileManager::new()?;
    mgr.disable()?;
    log::info!("[tip_manager] TIP 已禁用");
    Ok(())
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check -p tip_manager`
Expected: 编译通过，无 `todo!()` 警告

- [ ] **Step 3: 运行 tip_manager 单元测试**

Run: `cargo test -p tip_manager`
Expected: 编译通过（目前只有 mock 模块，后续会加测试）

- [ ] **Step 4: 提交**

```bash
git add windows/tip_manager/src/lib.rs
git commit -m "✨ feat(tip_manager): 实现 TIP 全生命周期公开 API

- install: 注册表写入 + COM RegisterProfile + EnableProfile
- uninstall: COM DisableProfile + UnregisterProfile + 注册表清理
- enable/disable: 纯 COM 调用"
```

---

### Task 7: 迁移 im_engine 注册入口到 tip_manager

**Files:**
- Modify: `windows/im_engine/src/lib.rs`
- Delete: `windows/im_engine/src/registrar.rs`

- [ ] **Step 1: 删除 registrar.rs**

删除文件 `windows/im_engine/src/registrar.rs`。

- [ ] **Step 2: 修改 lib.rs 中的 DllRegisterServer / DllUnregisterServer**

在 `windows/im_engine/src/lib.rs` 中，找到 `DllRegisterServer` 和 `DllUnregisterServer` 函数。

将 `DllRegisterServer` 函数体改为：

```rust
#[no_mangle]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    let dll_path = match get_this_dll_path() {
        Ok(p) => p,
        Err(_) => return HRESULT(-1),
    };
    match ::tip_manager::install(&dll_path) {
        Ok(()) => HRESULT(0),
        Err(e) => {
            log::error!("DllRegisterServer 失败: {e}");
            HRESULT(-1)
        }
    }
}
```

将 `DllUnregisterServer` 函数体改为：

```rust
#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    match ::tip_manager::uninstall() {
        Ok(()) => HRESULT(0),
        Err(e) => {
            log::error!("DllUnregisterServer 失败: {e}");
            HRESULT(-1)
        }
    }
}
```

需要保留 `get_this_dll_path()` 辅助函数（用 `GetModuleFileNameW` 获取 DLL 自身路径）。如果该函数原来在 `registrar.rs` 中，将其移到 `lib.rs`。

`get_this_dll_path()` 实现：

```rust
fn get_this_dll_path() -> Result<String, windows::core::Error> {
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
    let mut buf = vec![0u16; 260];
    let len = unsafe { GetModuleFileNameW(None, &mut buf) as usize };
    if len == 0 {
        return Err(windows::core::Error::from_win32());
    }
    Ok(String::from_utf16_lossy(&buf[..len]))
}
```

- [ ] **Step 3: 移除 im_engine 的 parking_lot 依赖（如不再需要）**

检查 `im_engine/Cargo.toml`——`parking_lot` 如果仅被 `registrar.rs` 的 `MODULE_HANDLE` 使用，现在可以移除。但检查其他文件（`lib.rs`、`text_service.rs`）是否仍使用 `parking_lot`。

`lib.rs` 中 `ENGINE` 使用 `OnceLock`（标准库），不是 `parking_lot`。确认后保持或移除。

- [ ] **Step 4: 编译验证**

Run: `cargo check -p im_engine`
Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add windows/im_engine/
git commit -m "♻️ refactor(im_engine): 注册入口迁移至 tip_manager

- 删除 registrar.rs，逻辑已移入 tip_manager
- DllRegisterServer/DllUnregisterServer 改为委托 tip_manager::install/uninstall
- get_this_dll_path 辅助函数留在 lib.rs"
```

---

### Task 8: 编写 tip_manager 单元测试

**Files:**
- Modify: `windows/tip_manager/src/detect.rs`（新增 tests 模块）

- [ ] **Step 1: 在 detect.rs 底部添加 tests 模块**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::mock::MockProfileManager;
    use crate::TipStatus;
    use std::cell::Cell;

    // 注意：is_registry_present() 在测试环境中通常返回 false
    // （没有注册表项），所以以下测试主要验证 COM 层的分支逻辑。

    #[test]
    fn test_detect_not_installed_when_no_registry() {
        // 无注册表 → 直接返回 NotInstalled，不调 COM
        let mock = MockProfileManager::new(false, false);
        let status = detect_status_impl(Some(&mock));
        assert_eq!(status, TipStatus::NotInstalled);
    }

    // registry_present 的测试需要实际写注册表，仅在集成/手动测试中验证。
    // 以下测试通过直接测试 detect_status_impl 的逻辑分支覆盖 COM 路径：
    // （不能 mock is_registry_present，但可以验证函数签名和类型正确性）
}
```

- [ ] **Step 2: 在 profile.rs 的 mock 模块添加测试**

在 `profile.rs` 的 `#[cfg(test)] mod mock { ... }` 内部已有 `MockProfileManager`，补充测试：

```rust
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
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p tip_manager`
Expected: 所有测试 PASS

- [ ] **Step 4: 提交**

```bash
git add windows/tip_manager/src/
git commit -m "🧪 test(tip_manager): 添加 MockProfileManager 与状态检测单元测试"
```

---

### Task 9: 在 settings 中新增「输入法管理」面板 UI

**Files:**
- Create: `windows/settings/src/panels/tip_manager.rs`
- Modify: `windows/settings/src/panels/mod.rs`
- Modify: `windows/settings/src/state.rs`
- Modify: `windows/settings/src/app.rs`
- Modify: `windows/settings/Cargo.toml`

- [ ] **Step 1: 添加 tip_manager 依赖到 settings**

在 `windows/settings/Cargo.toml` 的 `[dependencies]` 段末尾新增：

```toml
tip_manager = { path = "../tip_manager" }
```

- [ ] **Step 2: 在 state.rs 中新增 tip_status 字段**

在 `AppState` 结构体中新增字段（放在 `scanned_tables` 之后）：

```rust
    /// TIP 当前安装与启用状态（启动时检测一次）。
    pub tip_status: tip_manager::TipStatus,
```

在 `AppState::load()` 函数的 `Self { ... }` 构造末尾，`scanned_tables` 之后新增：

```rust
            tip_status: tip_manager::detect_status(),
```

- [ ] **Step 3: 在 panels/mod.rs 中新增 Panel::TipManager 变体**

```rust
use crate::state::{AppState, Panel};
```

改为：

```rust
use crate::state::{AppState, Panel};
pub mod tip_manager;
```

`Panel` 枚举新增变体：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Basic,
    Appearance,
    Dictionary,
    TipManager,  // ← 新增
    About,
}
```

`show_active` 函数的 `match` 分支新增：

```rust
        Panel::TipManager => tip_manager::show(ui, state),
```

- [ ] **Step 4: 编写 panels/tip_manager.rs（面板 UI）**

```rust
//! 输入法管理面板：TIP 状态显示与安装/启用/禁用/卸载操作。

use crate::state::AppState;
use eframe::egui::{self, Ui};
use tip_manager::TipStatus;

/// 渲染「输入法管理」面板。
pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("输入法管理");
    ui.add_space(8.0);

    match state.tip_status {
        TipStatus::NotInstalled => show_not_installed(ui, state),
        TipStatus::InstalledEnabled => show_installed_enabled(ui, state),
        TipStatus::InstalledDisabled => show_installed_disabled(ui, state),
        TipStatus::Unknown => show_unknown(ui, state),
    }
}

fn status_badge(ui: &mut Ui, color: egui::Color32, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.spacing_mut().item_spacing;
        ui.add_space(4.0);
        // 用一个小色块模拟指示灯
        ui.colored_label(color, "●");
        ui.label(text);
    });
}

fn show_not_installed(ui: &mut Ui, state: &mut AppState) {
    status_badge(ui, egui::Color32::GRAY, "未安装");
    ui.add_space(8.0);
    ui.label("MyWubi 输入法尚未安装到系统中。点击下方按钮完成安装，即可在语言设置中添加此输入法。");
    ui.add_space(12.0);

    if ui.button("安装输入法").clicked() {
        // 获取 settings.exe 同目录下的 im_engine.dll 路径
        let dll_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("im_engine.dll")))
            .and_then(|p| p.to_str().map(String::from));

        match dll_path {
            Some(path) => match tip_manager::install(&path) {
                Ok(()) => {
                    state.tip_status = tip_manager::detect_status();
                    state.status_msg = Some("✅ 输入法安装成功！使用 Win+Space 切换至此输入法。".into());
                }
                Err(e) => {
                    state.status_msg = Some(format!("❌ 安装失败: {e}"));
                }
            },
            None => {
                state.status_msg = Some("❌ 找不到 im_engine.dll，请确保它与 settings.exe 在同一目录。".into());
            }
        }
    }

    ui.add_space(8.0);
    ui.label("安装需要管理员权限。");
}

fn show_installed_enabled(ui: &mut Ui, state: &mut AppState) {
    status_badge(ui, egui::Color32::GREEN, "已安装 · 已启用");
    ui.add_space(8.0);
    ui.label("输入法运行正常。使用 Win+Space 切换至此输入法即可开始打字。");
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("禁用").clicked() {
            match tip_manager::disable() {
                Ok(()) => {
                    state.tip_status = tip_manager::detect_status();
                    state.status_msg = Some("✅ 输入法已禁用".into());
                }
                Err(e) => {
                    state.status_msg = Some(format!("❌ 禁用失败: {e}"));
                }
            }
        }
        if ui.button("卸载").clicked() {
            match tip_manager::uninstall() {
                Ok(()) => {
                    state.tip_status = tip_manager::detect_status();
                    state.status_msg = Some("✅ 输入法已卸载".into());
                }
                Err(e) => {
                    state.status_msg = Some(format!("❌ 卸载失败: {e}"));
                }
            }
        }
    });
}

fn show_installed_disabled(ui: &mut Ui, state: &mut AppState) {
    status_badge(ui, egui::Color32::from_rgb(255, 152, 0), "已安装 · 已禁用");
    ui.add_space(8.0);
    ui.label("输入法已安装但当前被禁用。启用后可在语言设置中选择。");
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("启用").clicked() {
            match tip_manager::enable() {
                Ok(()) => {
                    state.tip_status = tip_manager::detect_status();
                    state.status_msg = Some("✅ 输入法已启用".into());
                }
                Err(e) => {
                    state.status_msg = Some(format!("❌ 启用失败: {e}"));
                }
            }
        }
        if ui.button("卸载").clicked() {
            match tip_manager::uninstall() {
                Ok(()) => {
                    state.tip_status = tip_manager::detect_status();
                    state.status_msg = Some("✅ 输入法已卸载".into());
                }
                Err(e) => {
                    state.status_msg = Some(format!("❌ 卸载失败: {e}"));
                }
            }
        }
    });
}

fn show_unknown(ui: &mut Ui, state: &mut AppState) {
    status_badge(ui, egui::Color32::RED, "状态异常");
    ui.add_space(8.0);
    ui.label("检测到不完整的安装状态。建议尝试修复或完全卸载后重新安装。");
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        if ui.button("修复安装").clicked() {
            let dll_path = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("im_engine.dll")))
                .and_then(|p| p.to_str().map(String::from));

            if let Some(path) = dll_path {
                match tip_manager::install(&path) {
                    Ok(()) => {
                        state.tip_status = tip_manager::detect_status();
                        state.status_msg = Some("✅ 修复成功".into());
                    }
                    Err(e) => {
                        state.status_msg = Some(format!("❌ 修复失败: {e}"));
                    }
                }
            }
        }
        if ui.button("完全卸载").clicked() {
            match tip_manager::uninstall() {
                Ok(()) => {
                    state.tip_status = tip_manager::detect_status();
                    state.status_msg = Some("✅ 已完全卸载".into());
                }
                Err(e) => {
                    state.status_msg = Some(format!("❌ 卸载失败: {e}"));
                }
            }
        }
    });
}
```

- [ ] **Step 5: 在 app.rs 侧边栏中新增导航项**

在 `app.rs` 的 `show_sidebar` 函数中，`Panel::Dictionary` 和 `Panel::About` 之间新增：

```rust
                nav_item(ui, &mut self.state, Panel::TipManager, "输入法管理");
```

- [ ] **Step 6: 编译验证**

Run: `cargo check -p settings`
Expected: 编译通过

- [ ] **Step 7: 提交**

```bash
git add windows/settings/
git commit -m "✨ feat(settings): 新增「输入法管理」面板

- 侧边栏新增第 5 个导航项
- 面板按 TIP 状态分 4 种展示：未安装/已启用/已禁用/异常
- 支持安装、卸载、启用、禁用操作
- 启动时自动检测 TIP 状态"
```

---

### Task 10: 嵌入 requireAdministrator manifest 与管理员检查

**Files:**
- Create: `windows/settings/build.rs`
- Modify: `windows/settings/Cargo.toml`（新增 winres 依赖）
- Modify: `windows/settings/src/main.rs`

- [ ] **Step 1: 添加 winres 依赖**

在 `windows/settings/Cargo.toml` 的 `[target.'cfg(windows)'.dependencies]` 段新增：

```toml
winres = "0.1"
```

- [ ] **Step 2: 编写 build.rs**

```rust
fn main() {
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let mut res = winres::WindowsResource::new();
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
        <requestedPrivileges>
            <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
        </requestedPrivileges>
    </security>
</trustInfo>
</assembly>
"#,
        );
        res.compile().unwrap();
    }
}
```

- [ ] **Step 3: 在 main.rs 中添加管理员二次确认**

在 `main.rs` 的 `fn main()` 函数开头、日志初始化之后，新增：

```rust
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::IsUserAnAdmin;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR};
        if !unsafe { IsUserAnAdmin() }.as_bool() {
            unsafe {
                MessageBoxW(
                    None,
                    windows::core::w!("MyWubi 设置需要管理员权限才能管理输入法。请以管理员身份重新运行。"),
                    windows::core::w!("权限不足"),
                    MB_ICONERROR,
                );
            }
            std::process::exit(1);
        }
    }
```

- [ ] **Step 4: 编译验证**

Run: `cargo check -p settings`
Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add windows/settings/build.rs windows/settings/Cargo.toml windows/settings/src/main.rs
git commit -m "✨ feat(settings): 嵌入 requireAdministrator manifest

- build.rs 通过 winres 嵌入管理员权限清单
- main.rs 启动时二次确认管理员身份，非管理员弹框退出"
```

---

### Task 11: 全工作区编译与最终验证

**Files:** 无新建

- [ ] **Step 1: 全工作区编译检查**

Run: `cargo check --workspace`
Expected: 所有 crate 编译通过（含 tip_manager、im_engine、settings、core_engine）

- [ ] **Step 2: 运行所有测试**

Run: `cargo test --workspace`
Expected: 所有已有测试 + 新增测试 PASS（注意 im_engine 测试可能需要 Windows 环境，非 Windows 下跳过）

- [ ] **Step 3: 提交（如有遗漏变更）**

```bash
git status
# 如有遗漏，git add + commit
```

---

### Task 12: ROADMAP 更新

**Files:**
- Modify: `ROADMAP.md`

- [ ] **Step 1: 将已完成任务从「开发中」移至「已完成」**

在 ROADMAP.md 中找到以下内容（约第 51-58 行）：

```markdown
- [ ] **DLL 注册与反注册脚本及 Hook 开发**
    - [ ] 编写 `reg_script`（基于 `regsvr32` 或直接操作注册表注册 TSF 类 ID）
    - [ ] 编写 Rust 注册逻辑，并在 DLL 导出 `DllRegisterServer` 和 `DllUnregisterServer`
    - [ ] **在设置工具中集成 TIP 注册/启用功能**
        - [ ] 通过 `ITfInputProcessorProfileMgr` COM 接口实现 TIP 的注册与启用
        - [ ] 实现输入法的安装、启用、禁用、卸载全生命周期管理
        - [ ] 设置工具启动时检测 TIP 注册状态并提示用户操作
```

替换为：

```markdown
- [x] **DLL 注册与反注册脚本及 Hook 开发**
    - [x] 编写 `reg_script`（基于 `regsvr32` 或直接操作注册表注册 TSF 类 ID）
    - [x] 编写 Rust 注册逻辑，并在 DLL 导出 `DllRegisterServer` 和 `DllUnregisterServer`
    - [x] **在设置工具中集成 TIP 注册/启用功能**
        - [x] 通过 `ITfInputProcessorProfileMgr` COM 接口实现 TIP 的注册与启用（替代注册表方案，绕过未签名 DLL 的"仅桌面"灰显限制）
        - [x] 实现输入法的安装、启用、禁用、卸载全生命周期管理
        - [x] 设置工具启动时检测 TIP 注册状态并提示用户操作
```

并将此块从「🚧 开发中」移到「✅ 已完成」区域。

- [ ] **Step 2: 提交**

```bash
git add ROADMAP.md
git commit -m "📚 docs: 更新 ROADMAP——TIP 管理集成标记为已完成"
```

---

## 执行顺序

Task 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12

每个 Task 依赖前一个 Task 的产物，必须按序执行。
