# TIP 管理器与设置集成设计规格

- **日期**: 2026-06-25
- **范围**: 新建 `tip_manager` crate + settings「输入法管理」面板 + im_engine 注册逻辑迁移
- **状态**: 待审核
- **关联 ROADMAP**: "DLL 注册与反注册脚本及 Hook 开发" → "在设置工具中集成 TIP 注册/启用功能"

---

## 1. 目标与非目标

### 目标

- 在 `windows/tip_manager` 下新建 crate，集中管理 Windows TIP 注册表写入 + `ITfInputProcessorProfileMgr` COM 调用
- 实现 TIP 全生命周期：安装（注册+启用）、卸载、启用、禁用
- 在 settings 新增第 5 个侧边栏面板「输入法管理」，提供状态检测与操作 UI
- 将 `im_engine::registrar` 的注册逻辑迁移到 `tip_manager`，`DllRegisterServer`/`DllUnregisterServer` 改为委托调用
- settings.exe 嵌入 `requireAdministrator` manifest，启动即管理员权限

### 非目标

- 不含非管理员提权流程（如 `runas` 按需提权、UAC 降级方案）
- 不含输入法图标/指示器自定义
- 不含多用户 Profile 管理
- 不含静默安装/命令行安装（属 Velopack Hook 范畴）
- 不含 Android 端 TIP 管理

---

## 2. 关键决策汇总

| 决策点 | 选择 | 理由 |
|---|---|---|
| COM 策略 | 纯 `ITfInputProcessorProfileMgr` | 绕过未签名 DLL "仅桌面"灰显；注册表仅在安装/卸载时写入 |
| 逻辑位置 | `windows/tip_manager` crate | Windows 专用，逻辑集中、可独立测试、im_engine 和 settings 均可复用 |
| UI 位置 | 新增第 5 个面板「输入法管理」 | 与现有 4 面板平级，独立清晰 |
| 管理员权限 | `requireAdministrator` manifest | 最简单，无需运行时提权逻辑 |
| 可测试性 | `TipProfileManager` trait + mock | COM 无法在 CI 中真实调用，trait 抽象解耦 |
| DLL 路径 | 安装时由 settings 传入 `im_engine.dll` 绝对路径（与 settings.exe 同目录） | 不依赖环境变量或搜索路径；便携部署简单 |

---

## 3. 架构与 Crate 结构

### 3.1 新增 `tip_manager` crate

```
MyWubi/
├── Cargo.toml                    # workspace members 增加 "windows/tip_manager"
├── windows/
│   ├── tip_manager/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                # 公开 API
│   │       ├── guids.rs              # CLSID/GUID 常量（从 im_engine 迁移）
│   │       ├── registrar.rs          # 注册表写入（从 im_engine 迁移 + 重构）
│   │       ├── profile.rs            # ITfInputProcessorProfileMgr COM 封装
│   │       ├── detect.rs             # 状态检测
│   │       └── error.rs              # TipManagerError
│   ├── im_engine/
│   │   └── src/
│   │       └── registrar.rs      # → 删除，DllRegisterServer 改为调用 tip_manager
│   └── settings/
│       └── src/
│           └── panels/
│               ├── mod.rs        # 新增 Panel::TipManager 变体
│               └── tip_manager.rs # 新增：输入法管理面板 UI
```

### 3.2 依赖关系

```
settings.exe ──→ tip_manager ──→ windows-rs (Registry + COM)
im_engine.dll ──→ tip_manager ──→ windows-rs (Registry + COM)
```

### 3.3 `tip_manager` 公开 API

```rust
pub enum TipStatus {
    NotInstalled,
    InstalledDisabled,
    InstalledEnabled,
    Unknown,
}

pub fn install(dll_path: &str) -> Result<(), TipManagerError>;
pub fn uninstall() -> Result<(), TipManagerError>;
pub fn enable() -> Result<(), TipManagerError>;
pub fn disable() -> Result<(), TipManagerError>;
pub fn detect_status() -> TipStatus;
```

---

## 4. TIP 生命周期与状态机

```
                    install()
  NotInstalled ────────────────→ InstalledEnabled
       ↑                              │
       │                    disable() │  │ enable()
       │                              ↓  ↓
       │                        InstalledDisabled
       │                              │
       └────────── uninstall() ───────┘
```

### 4.1 `install(dll_path)` 步骤

1. 注册表写入 CLSID 子树（`HKCR\CLSID\{...}`）
   - `(Default)` = `TEXT_SERVICE_NAME`
   - `InprocServer32\(Default)` = `dll_path`, `ThreadingModel` = `Apartment`
   - `ProgID\(Default)` = `MyWubi.TextService.1`
   - `Implemented Categories\{CATID_TIP}` 子键
2. 注册表写入 TIP 子树（`HKLM\SOFTWARE\Microsoft\CTF\TIP\{...}`）
   - `(Default)` = `TEXT_SERVICE_NAME`
   - `Display Description`、`EnableCompatibleTsf`（DWORD=1）
   - `Category\Category\{CATID_TIP}`、`Category\Category\{CATID_KEYBOARD}`
   - `LanguageProfile\0x00000804\{GUID_PROFILE}`（Description、IconFile、IconIndex、Enable=1）
   - `CLSID` 子键
3. `CoCreateInstance(CLSID_TF_InputProcessorProfileMgr)` → `ITfInputProcessorProfileMgr`
4. `RegisterProfile()` 向系统注册 Profile
5. `EnableProfile()` 启用
6. 每步记录日志（`log::info!` / `log::error!`）

### 4.2 `uninstall()` 步骤

1. `DisableProfile()`（如果已启用）
2. `UnregisterProfile()`
3. `RegDeleteTreeW(HKCR\CLSID\{...})`
4. `RegDeleteTreeW(HKLM\...\CTF\TIP\{...})`
5. 尽力而为清理，失败只记警告不中断

### 4.3 `enable()` / `disable()`

纯 COM 调用，不动注册表。

### 4.4 `detect_status()` 检测逻辑

| 检查项 | 方法 | 结果 |
|---|---|---|
| `HKCR\CLSID\{CLSID}\InprocServer32` 是否存在 | `RegOpenKeyExW` | 不存在 → `NotInstalled` |
| `ITfInputProcessorProfileMgr::IsEnabledProfile()` | COM | `true` → `InstalledEnabled` |
| | | `false` → `InstalledDisabled` |
| 注册表残缺 / COM 不可用 | | → `Unknown` |

---

## 5. 可测试性设计

### 5.1 `TipProfileManager` trait

```rust
pub trait TipProfileManager {
    fn is_enabled(&self) -> Result<bool, TipManagerError>;
    fn enable(&self) -> Result<(), TipManagerError>;
    fn disable(&self) -> Result<(), TipManagerError>;
    fn register(&self) -> Result<(), TipManagerError>;
    fn unregister(&self) -> Result<(), TipManagerError>;
}
```

- `ComProfileManager`：真实 `ITfInputProcessorProfileMgr` 实现
- `MockProfileManager`：测试用，可编程返回值

状态检测和 COM 操作函数接受 `&dyn TipProfileManager` 参数，测试时注入 mock。

---

## 6. Settings UI 面板设计

### 6.1 侧边栏

新增第 5 个导航项「输入法管理」，与其他 4 个面板并列。

### 6.2 面板内容（按状态分支）

**未安装（`NotInstalled`）：**
- 灰色状态指示灯 + "未安装"文字
- 说明文字："MyWubi 输入法尚未安装到系统中"
- 「安装输入法」按钮（主色调，全宽）
- 底部提示："安装需要管理员权限"

**已安装·已启用（`InstalledEnabled`）：**
- 绿色状态指示灯 + "已安装 · 已启用"文字
- 信息卡片：DLL 路径、CLSID（只读展示）
- 操作按钮：「禁用」（橙色）、「卸载」（红色）

**已安装·已禁用（`InstalledDisabled`）：**
- 橙色状态指示灯 + "已安装 · 已禁用"文字
- 说明文字："输入法已安装但当前被禁用"
- 操作按钮：「启用」（绿色）、「卸载」（红色）

**未知异常（`Unknown`）：**
- 红色状态指示灯 + "状态异常"
- 说明文字："检测到不完整的安装状态"
- 操作按钮：「修复安装」、「完全卸载」

### 6.3 交互细节

- 启动时自动调用 `detect_status()`，刷新面板状态
- 操作执行中显示 `egui::Spinner` + 进度文字（如"正在安装…"）
- 操作完成后刷新状态，状态栏显示成功/失败消息（绿色/红色）
- 失败时提供"查看详情"可展开区域显示原始错误
- 卸载操作前弹出确认对话框："确定要卸载 MyWubi 输入法吗？"

### 6.4 代码位置

- `Panel` 枚举新增 `TipManager` 变体（`panels/mod.rs`）
- 新增 `panels/tip_manager.rs`，函数签名：`pub fn show(ui: &mut Ui, state: &mut AppState)`

---

## 7. 错误处理

### 7.1 错误类型

```rust
pub enum TipManagerError {
    Registry(String),         // 注册表操作失败
    Com(String),              // COM 调用失败
    DllNotFound(PathBuf),     // im_engine.dll 路径无效
    AccessDenied,             // 非管理员运行
    InconsistentState(String), // 注册表与 COM 状态不一致
}
```

### 7.2 各场景处理

| 场景 | 处理 |
|---|---|
| 安装时注册表写入失败 | 回滚已写入 key，返回 `Registry` 错误 |
| 安装时 COM RegisterProfile 失败 | 标记 `Unknown` 状态，UI 提示"部分安装，请重试" |
| 卸载时注册表删除失败 | 尽力而为，记录警告，返回部分成功 |
| enable/disable COM 失败 | 直接返回错误，UI 显示重试按钮 |
| 非管理员运行 settings | `IsUserAnAdmin()` 检测，弹消息框提示并退出 |
| 检测到 `Unknown` 状态 | UI 提供"修复安装"（重跑 install）和"完全卸载"两个选项 |

---

## 8. 管理员权限

- `windows/settings/build.rs` 通过 `winres` 嵌入 manifest：
  ```xml
  <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
  ```
- `main.rs` 启动时调用 `shell32::IsUserAnAdmin()` 二次确认
- 非管理员 → `MessageBoxW` 弹框："MyWubi 设置需要管理员权限才能管理输入法。请以管理员身份重新运行。" → 退出

---

## 9. im_engine 兼容性迁移

### 9.1 变更清单

| 文件 | 操作 |
|---|---|
| `windows/im_engine/src/guids.rs` | **迁移到 `tip_manager/src/guids.rs`**，im_engine 改为 `use tip_manager::guids` |
| `windows/im_engine/src/registrar.rs` | **删除**（逻辑迁移到 `tip_manager`） |
| `windows/im_engine/src/lib.rs` | `DllRegisterServer` 改为调用 `tip_manager::install()` |
| `windows/im_engine/src/lib.rs` | `DllUnregisterServer` 改为调用 `tip_manager::uninstall()` |
| `windows/im_engine/src/factory.rs` | 更新 `use crate::guids` → `use tip_manager::guids` |
| `windows/im_engine/src/text_service.rs` | 更新 `use crate::guids` → `use tip_manager::guids`（如有引用） |
| `windows/im_engine/Cargo.toml` | 新增 `tip_manager` 依赖 |

### 9.2 `regsvr32` 兼容

- `regsvr32 im_engine.dll` 仍然工作——`DllRegisterServer` 内部调用 `tip_manager::install(dll_path)`，dll_path 通过 `GetModuleFileNameW` 获取
- Velopack 安装 Hook 也可直接调用 `tip_manager::install()`，无需加载 DLL

---

## 10. 测试策略

| 层级 | 测试点 | 方式 |
|---|---|---|
| `tip_manager::registrar` | 注册表 key 路径拼接正确性 | 纯函数单元测试 |
| `tip_manager::detect` | 各状态分支（注入 mock） | 依赖注入 + mock |
| `tip_manager::profile` | COM GUID 常量 | 编译期 const 断言 |
| `tip_manager::error` | Display / Error trait | 标准测试 |
| `settings::panels::tip_manager` | 不同 TipStatus 渲染正确按钮 | 构造状态验证 |
| 手动验收 | 干净 Win VM 完整安装→启用→禁用→卸载流程 | 手动 |
| im_engine 兼容 | `regsvr32` 注册/反注册仍正常 | 手动 |

---

## 11. 新增依赖

### `windows/tip_manager/Cargo.toml`
```toml
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
```

### `windows/settings/Cargo.toml` 变更
```toml
[dependencies]
tip_manager = { path = "../tip_manager" }
# winres 用于嵌入 manifest
```

### `windows/settings/build.rs` 新增
```rust
// 嵌入 requireAdministrator manifest
fn main() {
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        let mut res = winresource::WindowsResource::new();
        res.set_manifest(r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#);
        res.compile().unwrap();
    }
}
```

---

## 12. 后续 ROADMAP 衍生任务

- Velopack 安装 Hook 调用 `tip_manager::install()` 实现自动注册
- Velopack 卸载 Hook 调用 `tip_manager::uninstall()` 实现自动清理
- 所有独立安装脚本已移除（`register_tip.bat`、`enable_tip.ps1`、`reg_script.ps1` 等），统一使用 `settings.exe` 的「输入法管理」面板
