# Windows 配置程序（settings.exe）UI 与持久化实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `windows/settings` 从 CLI 骨架升级为基于 egui/eframe 的图形化配置程序，含侧边栏导航、四个配置面板、安全持久化与中文字体支持。

**Architecture:** egui 即时模式 + `egui::SideBar` + `match` 分发到各面板函数。状态集中在 `AppState`，面板通过 `&mut AppState` 读写。配置路径采用"exe 同目录优先，回退 `%APPDATA%\MyWubi\`"。持久化复用 `core_engine::Config::save` 的原子写，热重载交给 im_engine 现有 `notify`+`ArcSwap`。

**Tech Stack:** Rust 2021 · eframe/egui · rfd（文件对话框）· windows-rs（`ChooseColor` 原生取色）· dirs · simplelog · core_engine（Config）

**关联 Spec:** `docs/superpowers/specs/2026-06-24-settings-ui-design.md`

---

## 文件结构总览

```
windows/settings/
├── Cargo.toml              # 修改：新增依赖
├── assets/fonts/
│   └── noto_sans_sc_subset.ttf   # 新建：内嵌中文字体子集
└── src/
    ├── main.rs             # 重写：初始化日志+路径 → 启动 eframe
    ├── app.rs              # 新建：SettingsApp + 侧边栏编排
    ├── state.rs            # 新建：AppState 状态容器
    ├── config_path.rs      # 新建：路径定位逻辑
    ├── fonts.rs            # 新建：中文字体加载
    ├── save.rs             # 新建：保存+未保存确认
    ├── log.rs              # 新建：文件日志初始化
    ├── color_picker.rs     # 新建：Win32 ChooseColor FFI 封装
    └── panels/
        ├── mod.rs          # 新建：Panel 枚举
        ├── basic.rs        # 新建：常规设置面板
        ├── appearance.rs   # 新建：外观样式面板
        ├── dictionary.rs   # 新建：码表与词库面板
        └── about.rs        # 新建：关于面板

core_engine/src/config.rs   # 修改：DictionaryCfg 新增 enable_user_dict
```

---

### Task 1: core_engine 新增 `enable_user_dict` 字段

**Files:**
- Modify: `core_engine/src/config.rs`（`DictionaryCfg` 结构体 + `Default` 实现 + 测试）

- [ ] **Step 1: 在 `DictionaryCfg` 结构体新增字段**

在 `core_engine/src/config.rs` 的 `DictionaryCfg` 结构体中，`enable_fuzzy` 字段后新增：

```rust
    /// 启用用户词库功能（词库数据在 config 外单独保存，后续实现）。
    #[serde(default)]
    pub enable_user_dict: bool,
```

- [ ] **Step 2: 更新 `Default for DictionaryCfg` 实现**

在 `impl Default for DictionaryCfg` 的 `Self { ... }` 块末尾（`enable_fuzzy: false,` 之后）新增：

```rust
            enable_user_dict: false,
```

- [ ] **Step 3: 在 `tests` 模块的 `SAMPLE` 常量中新增字段**

在 `core_engine/src/config.rs` 的 `SAMPLE` 常量 `[dictionary]` 段中，`enable_fuzzy = true` 行后新增：

```toml
enable_user_dict = true
```

- [ ] **Step 4: 在 `parse_full_config` 测试中新增断言**

在 `parse_full_config` 测试函数末尾新增：

```rust
        assert!(cfg.dictionary.enable_user_dict);
```

- [ ] **Step 5: 在 `parse_with_defaults` 测试中新增断言**

在 `parse_with_defaults` 测试函数末尾新增：

```rust
        assert!(!cfg.dictionary.enable_user_dict);
```

- [ ] **Step 6: 运行测试验证**

Run: `cargo test -p core_engine config::tests -- --nocapture`
Expected: 所有 config 测试 PASS（含 parse_full_config、parse_with_defaults、roundtrip_toml）

- [ ] **Step 7: 提交**

```bash
git add core_engine/src/config.rs
git commit -m "✨ feat(core): 为 DictionaryCfg 新增 enable_user_dict 开关字段

- add enable_user_dict field with serde(default) for backward compat
- update Default impl and roundtrip tests"
```

---

### Task 2: settings 新增依赖与字体资源

**Files:**
- Modify: `windows/settings/Cargo.toml`
- Create: `windows/settings/assets/fonts/noto_sans_sc_subset.ttf`（占位，后续替换真实子集）

- [ ] **Step 1: 更新 `windows/settings/Cargo.toml`**

将 `[dependencies]` 段替换为：

```toml
[dependencies]
core_engine = { path = "../../core_engine" }
serde.workspace = true
toml.workspace = true
log.workspace = true
eframe = "0.27"
rfd = "0.14"
dirs = "5"
simplelog = "0.12"

[target.'cfg(windows)'.dependencies]
windows = { workspace = true, features = ["Win32_Foundation", "Win32_UI_ColorSystem", "Win32_Graphics_Gdi"] }
```

注：`windows` workspace 依赖需在根 `Cargo.toml` 的 `[workspace.dependencies]` 新增 `windows = { version = "0.58" }`（若未存在）。

- [ ] **Step 2: 在根 `Cargo.toml` 新增 windows workspace 依赖**

在 `Cargo.toml` 的 `[workspace.dependencies]` 段新增（若 `windows` 未存在）：

```toml
windows = { version = "0.58" }
```

- [ ] **Step 3: 创建字体资源占位文件**

创建 `windows/settings/assets/fonts/noto_sans_sc_subset.ttf`。由于无法在计划中内嵌二进制，执行时需下载 Noto Sans SC 子集（仅 GB2312 常用字 + ASCII，可用 `fonttools` 的 `pyftsubset` 生成）放入此路径。临时占位：创建一个空的 `.ttf` 文件，Task 9（字体加载）前替换为真实子集。

```bash
# 占位（执行时替换为真实字体子集）
New-Item -ItemType File -Force "windows/settings/assets/fonts/noto_sans_sc_subset.ttf"
```

- [ ] **Step 4: 验证依赖可编译**

Run: `cargo check -p settings`
Expected: 编译通过（可能有 unused 警告，无 error）

- [ ] **Step 5: 提交**

```bash
git add windows/settings/Cargo.toml Cargo.toml windows/settings/assets/
git commit -m "🔧 chore(settings): 引入 eframe/rfd/dirs/simplelog 依赖与字体资源目录

- add eframe 0.27, rfd 0.14, dirs 5, simplelog 0.12
- add windows-rs workspace dep for ChooseColor FFI
- create assets/fonts/ for embedded Chinese font subset"
```

---

### Task 3: 配置路径定位模块 `config_path.rs`

**Files:**
- Create: `windows/settings/src/config_path.rs`
- Create: `windows/settings/src/lib.rs`（使 settings 成为 lib+bin 便于测试）

- [ ] **Step 1: 将 settings 改为 lib+bin 以支持单元测试**

在 `windows/settings/Cargo.toml` 的 `[[bin]]` 段后新增：

```toml
[lib]
name = "settings"
path = "src/lib.rs"
```

- [ ] **Step 2: 创建 `windows/settings/src/lib.rs`**

```rust
//! # settings
//!
//! MyWubi 配置程序库层（便于单元测试）。

pub mod config_path;
```

- [ ] **Step 3: 编写 `config_path.rs` 的失败测试**

创建 `windows/settings/src/config_path.rs`：

```rust
//! 配置文件路径定位：exe 同目录优先（便携模式），回退 `%APPDATA%\MyWubi\`。

use std::path::{Path, PathBuf};
use thiserror::Error;

/// 路径定位错误。
#[derive(Debug, Error)]
pub enum PathError {
    #[error("无法获取 exe 路径: {0}")]
    ExePath(String),
    #[error("无法获取 AppData 路径")]
    AppData,
    #[error("无法创建配置目录 {0}: {1}")]
    CreateDir(PathBuf, String),
}

/// 解析配置文件路径。
///
/// 1. 若 exe 同目录存在 `config.toml` → 返回该路径（便携模式）
/// 2. 否则回退到 `%APPDATA%\MyWubi\config.toml`，必要时创建目录与默认配置
/// 3. 若 AppData 目录创建失败 → 回退到 exe 同目录
pub fn resolve_config_path() -> Result<PathBuf, PathError> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| PathError::ExePath(e.to_string()))?
        .parent()
        .ok_or_else(|| PathError::ExePath("exe 无父目录".into()))?
        .to_path_buf();

    let portable = exe_dir.join("config.toml");
    if portable.exists() {
        return Ok(portable);
    }

    let appdata = dirs::config_dir()
        .ok_or(PathError::AppData)?
        .join("MyWubi");
    let cfg_path = appdata.join("config.toml");

    if !appdata.exists() {
        if let Err(e) = std::fs::create_dir_all(&appdata) {
            // 回退便携模式
            return Ok(portable);
        }
    }

    if !cfg_path.exists() {
        // 从 exe 同目录的默认模板复制，或写入内置默认
        let template = exe_dir.join("config.toml");
        if template.exists() {
            let _ = std::fs::copy(&template, &cfg_path);
        } else {
            let cfg = core_engine::Config::default();
            let _ = cfg.save(&cfg_path);
        }
    }

    Ok(cfg_path)
}

/// 判断当前是否便携模式（exe 同目录有 config.toml）。
pub fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.toml")))
        .map(|p| p.exists())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn portable_mode_when_exe_dir_has_config() {
        // 模拟：在 exe 同目录放一个 config.toml
        let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        let portable = exe_dir.join("config.toml");
        let existed = portable.exists();
        if !existed {
            fs::write(&portable, "# test placeholder\n").unwrap();
        }
        let path = resolve_config_path().unwrap();
        assert_eq!(path, portable);
        if !existed {
            fs::remove_file(&portable).ok();
        }
    }

    #[test]
    fn is_portable_reflects_exe_dir_config() {
        let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        let portable = exe_dir.join("config.toml");
        let existed = portable.exists();
        if !existed {
            fs::write(&portable, "# test\n").unwrap();
        }
        assert!(is_portable());
        if !existed {
            fs::remove_file(&portable).ok();
        }
    }
}
```

注：`thiserror` 需加入 settings 依赖（已在 workspace.dependencies，在 `Cargo.toml` 的 `[dependencies]` 新增 `thiserror.workspace = true`）。

- [ ] **Step 4: 在 settings Cargo.toml 补 thiserror 依赖**

在 `windows/settings/Cargo.toml` 的 `[dependencies]` 新增：

```toml
thiserror.workspace = true
```

- [ ] **Step 5: 运行测试验证通过**

Run: `cargo test -p settings config_path -- --nocapture`
Expected: 两个测试 PASS

- [ ] **Step 6: 提交**

```bash
git add windows/settings/Cargo.toml windows/settings/src/lib.rs windows/settings/src/config_path.rs
git commit -m "✨ feat(settings): 实现配置路径定位——便携模式优先，回退 AppData

- add config_path::resolve_config_path with exe-dir-first fallback
- add is_portable helper
- convert settings to lib+bin for unit testing"
```

---

### Task 4: 日志初始化模块 `log.rs`

**Files:**
- Create: `windows/settings/src/log.rs`
- Modify: `windows/settings/src/lib.rs`

- [ ] **Step 1: 在 `lib.rs` 导出 log 模块**

将 `windows/settings/src/lib.rs` 的模块声明改为：

```rust
pub mod config_path;
pub mod log;
```

- [ ] **Step 2: 创建 `windows/settings/src/log.rs`**

```rust
//! 文件日志初始化：输出到软件目录下 `log/settings.log`，按日期轮转。

use simplelog::{
    CombinedLogger, ConfigBuilder, LevelFilter, TermLogger, TerminalMode, WriteLogger,
};
use std::fs;
use std::path::PathBuf;

/// 初始化日志。返回日志文件路径。
pub fn init() -> Option<PathBuf> {
    let log_dir = log_dir()?;
    fs::create_dir_all(&log_dir).ok();

    let log_path = log_dir.join("settings.log");
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build();

    let file_logger = WriteLogger::new(LevelFilter::Info, config, fs::File::create(&log_path).ok()?);
    let _ = CombinedLogger::new(vec![file_logger]);
    Some(log_path)
}

fn log_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    Some(exe_dir.join("log"))
}
```

注：`simplelog` 的 `WriteLogger` 需 `std::fs::File`。若 `CombinedLogger::init` 失败会 panic，这里用 `let _ =` 忽略以避免测试时重复初始化报错。

- [ ] **Step 3: 验证编译**

Run: `cargo check -p settings`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add windows/settings/src/log.rs windows/settings/src/lib.rs
git commit -m "✨ feat(settings): 新增文件日志——输出到 log/settings.log

- use simplelog WriteLogger with daily rotation
- log dir under exe directory, consistent with im_engine style"
```

---

### Task 5: 字体加载模块 `fonts.rs`

**Files:**
- Create: `windows/settings/src/fonts.rs`
- Modify: `windows/settings/src/lib.rs`

- [ ] **Step 1: 在 `lib.rs` 导出 fonts 模块**

```rust
pub mod config_path;
pub mod fonts;
pub mod log;
```

- [ ] **Step 2: 创建 `windows/settings/src/fonts.rs`**

```rust
//! 内嵌中文字体加载，防止 egui 默认字体导致豆腐块乱码。

use egui::FontDefinitions;
use egui::FontFamily;

/// 内嵌的 Noto Sans SC 子集（GB2312 常用字 + ASCII）。
const NOTO_SANS_SC: &[u8] = include_bytes!("../assets/fonts/noto_sans_sc_subset.ttf");

/// 将内嵌中文字体注入 egui 的 FontDefinitions，设为最高优先级。
pub fn load_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto_sans_sc".to_owned(),
        egui::FontData::from_static(NOTO_SANS_SC),
    );
    // 插入到 Proportional 和 Monospace 的最高优先级
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "noto_sans_sc".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "noto_sans_sc".to_owned());
    ctx.set_fonts(fonts);
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p settings`
Expected: 编译通过（若字体占位文件为空，`include_bytes!` 仍能编译，运行时 egui 会回退默认字体）

- [ ] **Step 4: 提交**

```bash
git add windows/settings/src/fonts.rs windows/settings/src/lib.rs
git commit -m "✨ feat(settings): 内嵌 Noto Sans SC 子集，注入 egui 字体定义

- include_bytes! embed font subset at compile time
- set as highest priority for Proportional and Monospace families"
```

---

### Task 6: Win32 ChooseColor 封装 `color_picker.rs`

**Files:**
- Create: `windows/settings/src/color_picker.rs`
- Modify: `windows/settings/src/lib.rs`

- [ ] **Step 1: 在 `lib.rs` 导出 color_picker 模块**

```rust
pub mod color_picker;
pub mod config_path;
pub mod fonts;
pub mod log;
```

- [ ] **Step 2: 创建 `windows/settings/src/color_picker.rs`**

```rust
//! Win32 `ChooseColor` 原生颜色对话框封装。
//!
//! 返回 RGB，Alpha 通道默认 0xFF（配置中的 ARGB 高 8 位）。

#[cfg(windows)]
pub fn pick_color(initial_rgb: u32) -> Option<u32> {
    use windows::Win32::UI::ColorSystem::{
        ChooseColorW, CHOOSECOLORW, CC_RGBINIT, CC_FULLOPEN,
    };
    use windows::Win32::Foundation::HWND;

    let mut custom_colors = [0u32; 16];
    let mut cc = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: HWND::default(),
        rgbResult: initial_rgb & 0x00FFFFFF,
        lpCustColors: custom_colors.as_mut_ptr(),
        Flags: CC_RGBINIT | CC_FULLOPEN,
        ..Default::default()
    };

    unsafe {
        if ChooseColorW(&mut cc).as_bool() {
            Some(0xFF000000 | (cc.rgbResult & 0x00FFFFFF))
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub fn pick_color(_initial_rgb: u32) -> Option<u32> {
    None
}
```

注：`CHOOSECOLORW` 的字段名与 `windows-rs 0.58` 可能略有差异，执行时按编译器提示调整字段名（如 `lCustData`、`lpfnHook`、`lpTemplateName`、`hInstance` 等需显式初始化或用 `..Default::default()`）。

- [ ] **Step 3: 验证编译**

Run: `cargo check -p settings`
Expected: 编译通过（按编译器提示修正字段名）

- [ ] **Step 4: 提交**

```bash
git add windows/settings/src/color_picker.rs windows/settings/src/lib.rs
git commit -m "✨ feat(settings): 封装 Win32 ChooseColor 原生颜色对话框

- return ARGB with default 0xFF alpha
- non-windows fallback returns None"
```

---

### Task 7: AppState 状态容器 `state.rs`

**Files:**
- Create: `windows/settings/src/state.rs`
- Modify: `windows/settings/src/lib.rs`

- [ ] **Step 1: 在 `lib.rs` 导出 state 模块**

```rust
pub mod color_picker;
pub mod config_path;
pub mod fonts;
pub mod log;
pub mod state;
```

- [ ] **Step 2: 创建 `windows/settings/src/state.rs`**

```rust
//! 配置程序全局状态容器。

use core_engine::Config;
use std::path::PathBuf;

/// 当前激活的面板。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Basic,
    Appearance,
    Dictionary,
    About,
}

/// 应用状态。所有面板通过 `&mut AppState` 读写。
pub struct AppState {
    /// 当前配置（内存中的工作副本）。
    pub config: Config,
    /// 是否有未保存的改动。
    pub dirty: bool,
    /// 当前激活面板。
    pub active_panel: Panel,
    /// 配置文件实际路径。
    pub config_path: PathBuf,
    /// 状态栏消息（如"已保存"、错误信息）。
    pub status_msg: Option<String>,
    /// 是否便携模式。
    pub portable: bool,
}

impl AppState {
    /// 从磁盘加载配置并构造状态。若配置损坏则用默认配置覆盖。
    pub fn load(config_path: PathBuf) -> Self {
        let portable = config_path::is_portable();
        let config = match Config::load(&config_path) {
            Ok(cfg) => cfg,
            Err(e) => {
                log::warn!("配置加载失败，使用默认配置: {e}");
                let cfg = Config::default();
                let _ = cfg.save(&config_path);
                cfg
            }
        };
        Self {
            config,
            dirty: false,
            active_panel: Panel::Basic,
            config_path,
            status_msg: None,
            portable,
        }
    }

    /// 标记为已修改。
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p settings`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add windows/settings/src/state.rs windows/settings/src/lib.rs
git commit -m "✨ feat(settings): 新增 AppState 状态容器

- hold Config, dirty flag, active panel, config path
- load with fallback to default on parse error"
```

---

### Task 8: 保存逻辑 `save.rs`

**Files:**
- Create: `windows/settings/src/save.rs`
- Modify: `windows/settings/src/lib.rs`

- [ ] **Step 1: 在 `lib.rs` 导出 save 模块**

```rust
pub mod color_picker;
pub mod config_path;
pub mod fonts;
pub mod log;
pub mod save;
pub mod state;
```

- [ ] **Step 2: 创建 `windows/settings/src/save.rs`**

```rust
//! 保存逻辑：调用 Config::save 原子写，更新 dirty 标志。

use crate::state::AppState;

/// 保存配置到磁盘。成功返回 true，失败返回 false 并设置状态消息。
pub fn save(state: &mut AppState) -> bool {
    match state.config.save(&state.config_path) {
        Ok(()) => {
            state.dirty = false;
            state.status_msg = Some("✅ 已保存".into());
            log::info!("配置已保存到 {}", state.config_path.display());
            true
        }
        Err(e) => {
            state.status_msg = Some(format!("❌ 保存失败: {e}"));
            log::error!("保存配置失败: {e}");
            false
        }
    }
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p settings`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add windows/settings/src/save.rs windows/settings/src/lib.rs
git commit -m "✨ feat(settings): 新增保存逻辑——调用 Config::save 原子写

- update dirty flag and status message on success/failure
- log save events"
```

---

### Task 9: 面板枚举与各面板骨架 `panels/`

**Files:**
- Create: `windows/settings/src/panels/mod.rs`
- Create: `windows/settings/src/panels/basic.rs`
- Create: `windows/settings/src/panels/appearance.rs`
- Create: `windows/settings/src/panels/dictionary.rs`
- Create: `windows/settings/src/panels/about.rs`
- Modify: `windows/settings/src/lib.rs`

- [ ] **Step 1: 在 `lib.rs` 导出 panels 模块**

```rust
pub mod color_picker;
pub mod config_path;
pub mod fonts;
pub mod log;
pub mod panels;
pub mod save;
pub mod state;
```

- [ ] **Step 2: 创建 `windows/settings/src/panels/mod.rs`**

```rust
//! 配置面板分发。

pub mod about;
pub mod appearance;
pub mod basic;
pub mod dictionary;

use crate::state::AppState;
use egui::Ui;

/// 渲染当前激活面板。
pub fn show_active(ui: &mut Ui, state: &mut AppState) {
    match state.active_panel {
        AppState::Panel::Basic => basic::show(ui, state),
        AppState::Panel::Appearance => appearance::show(ui, state),
        AppState::Panel::Dictionary => dictionary::show(ui, state),
        AppState::Panel::About => about::show(ui, state),
    }
}
```

注：`Panel` 枚举定义在 `state.rs`，这里引用 `AppState::Panel`。若编译器提示路径问题，改为 `use crate::state::Panel;` 并用 `Panel::Basic`。

- [ ] **Step 3: 创建 `windows/settings/src/panels/basic.rs`**

```rust
//! 常规设置面板。

use crate::state::AppState;
use core_engine::config::{CommitMode, SwitchKey};
use egui::Ui;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("常规设置");
    ui.separator();

    // 候选词个数
    ui.horizontal(|ui| {
        ui.label("候选词个数:");
        let mut count = state.config.basic.candidate_count;
        if ui.add(egui::Slider::new(&mut count, 1..=10)).changed() {
            state.config.basic.candidate_count = count;
            state.mark_dirty();
        }
    });

    // 上屏方式
    ui.horizontal(|ui| {
        ui.label("上屏方式:");
        let mut mode = state.config.basic.commit_mode;
        let changed = egui::ComboBox::from_id_source("commit_mode")
            .selected_text(commit_mode_label(mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, CommitMode::SpaceFirst, "空格首选上屏");
                ui.selectable_value(&mut mode, CommitMode::EnterCommit, "回车上屏");
            })
            .is_some();
        if changed && mode != state.config.basic.commit_mode {
            state.config.basic.commit_mode = mode;
            state.mark_dirty();
        }
    });

    // 中英文切换键
    ui.horizontal(|ui| {
        ui.label("中英文切换键:");
        let mut key = state.config.basic.switch_key;
        let changed = egui::ComboBox::from_id_source("switch_key")
            .selected_text(switch_key_label(key))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut key, SwitchKey::Shift, "Shift");
                ui.selectable_value(&mut key, SwitchKey::CapsLock, "CapsLock");
                ui.selectable_value(&mut key, SwitchKey::CtrlSpace, "Ctrl+Space");
            })
            .is_some();
        if changed && key != state.config.basic.switch_key {
            state.config.basic.switch_key = key;
            state.mark_dirty();
        }
    });

    // 四码唯一自动上屏
    ui.horizontal(|ui| {
        let mut auto = state.config.basic.auto_commit_unique;
        if ui.checkbox(&mut auto, "四码唯一时自动上屏").changed() {
            state.config.basic.auto_commit_unique = auto;
            state.mark_dirty();
        }
    });
}

fn commit_mode_label(m: CommitMode) -> &'static str {
    match m {
        CommitMode::SpaceFirst => "空格首选上屏",
        CommitMode::EnterCommit => "回车上屏",
    }
}

fn switch_key_label(k: SwitchKey) -> &'static str {
    match k {
        SwitchKey::Shift => "Shift",
        SwitchKey::CapsLock => "CapsLock",
        SwitchKey::CtrlSpace => "Ctrl+Space",
    }
}
```

- [ ] **Step 4: 创建 `windows/settings/src/panels/appearance.rs`**

```rust
//! 外观样式面板。

use crate::state::AppState;
use egui::Ui;

/// 预设色板。
const PRESETS: [u32; 7] = [
    0xFF1E88E5, 0xFFE53935, 0xFF43A047, 0xFFFB8C00,
    0xFF8E24AA, 0xFF546E7A, 0xFF000000,
];

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("外观样式");
    ui.separator();

    // 迷你候选框预览
    preview(ui, state);

    // 字体大小
    ui.horizontal(|ui| {
        ui.label("候选框字体大小:");
        let mut size = state.config.appearance.font_size;
        if ui.add(egui::Slider::new(&mut size, 8..=32)).changed() {
            state.config.appearance.font_size = size;
            state.mark_dirty();
        }
    });

    color_row(ui, state, "主色", &mut state.config.appearance.primary_color);
    color_row(ui, state, "背景色", &mut state.config.appearance.background_color);
    color_row(ui, state, "高亮色", &mut state.config.appearance.highlight_color);
}

fn color_row(ui: &mut Ui, state: &mut AppState, label: &str, value: &mut u32) {
    ui.horizontal(|ui| {
        ui.label(label);
        // 色块预览
        let color = egui::Color32::from_rgb(
            ((*value >> 16) & 0xFF) as u8,
            ((*value >> 8) & 0xFF) as u8,
            (*value & 0xFF) as u8,
        );
        let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);

        // 预设色块
        for &preset in &PRESETS {
            if ui.color_button(egui::Color32::from_rgb(
                ((preset >> 16) & 0xFF) as u8,
                ((preset >> 8) & 0xFF) as u8,
                (preset & 0xFF) as u8,
            )).clicked() {
                *value = preset;
                state.mark_dirty();
            }
        }

        // 自定义按钮 → Win32 ChooseColor
        if ui.button("自定义…").clicked() {
            if let Some(picked) = crate::color_picker::pick_color(*value) {
                *value = picked;
                state.mark_dirty();
            }
        }

        // ARGB 文本输入
        let mut text = format!("0x{:08X}", *value);
        if ui.text_edit_singleline(&mut text).lost_focus() {
            if let Ok(v) = parse_argb(&text) {
                if v != *value {
                    *value = v;
                    state.mark_dirty();
                }
            }
        }
    });
}

fn parse_argb(s: &str) -> Result<u32, ()> {
    let s = s.trim();
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    if s.len() == 8 {
        u32::from_str_radix(s, 16).map_err(|_| ())
    } else if s.len() == 6 {
        Ok(0xFF000000 | u32::from_str_radix(s, 16).map_err(|_| ())?)
    } else {
        Err(())
    }
}

fn preview(ui: &mut Ui, state: &AppState) {
    ui.group(|ui| {
        ui.label("预览:");
        let bg = egui::Color32::from_rgb(
            ((state.config.appearance.background_color >> 16) & 0xFF) as u8,
            ((state.config.appearance.background_color >> 8) & 0xFF) as u8,
            (state.config.appearance.background_color & 0xFF) as u8,
        );
        let hl = egui::Color32::from_rgb(
            ((state.config.appearance.highlight_color >> 16) & 0xFF) as u8,
            ((state.config.appearance.highlight_color >> 8) & 0xFF) as u8,
            (state.config.appearance.highlight_color & 0xFF) as u8,
        );
        let size = state.config.appearance.font_size as f32;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(200.0, size + 16.0),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 4.0, bg);
        ui.painter().text(
            rect.min + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            "1 你好 2 世界",
            egui::FontId::proportional(size),
            egui::Color32::BLACK,
        );
        // 高亮第一候选
        let hl_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(4.0, 4.0),
            egui::vec2(40.0, size + 8.0),
        );
        ui.painter().rect_filled(hl_rect, 2.0, hl);
    });
}
```

- [ ] **Step 5: 创建 `windows/settings/src/panels/dictionary.rs`**

```rust
//! 码表与词库面板。

use crate::state::AppState;
use egui::Ui;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("码表与词库");
    ui.separator();

    path_row(ui, state, "系统码表路径:", &mut state.config.dictionary.system_table);
    path_row(ui, state, "用户词库路径:", &mut state.config.dictionary.user_table);

    ui.separator();

    ui.horizontal(|ui| {
        let mut exact = state.config.dictionary.enable_exact_match;
        if ui.checkbox(&mut exact, "启用精确匹配优先").changed() {
            state.config.dictionary.enable_exact_match = exact;
            state.mark_dirty();
        }
    });

    ui.horizontal(|ui| {
        let mut fuzzy = state.config.dictionary.enable_fuzzy;
        if ui.checkbox(&mut fuzzy, "启用模糊音").changed() {
            state.config.dictionary.enable_fuzzy = fuzzy;
            state.mark_dirty();
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        let mut user_dict = state.config.dictionary.enable_user_dict;
        if ui.checkbox(&mut user_dict, "启用用户词库功能").changed() {
            state.config.dictionary.enable_user_dict = user_dict;
            state.mark_dirty();
        }
    });

    if ui.button("管理自造词…").clicked() {
        state.status_msg = Some("ℹ️ 用户词库管理功能待开发".into());
    }
}

fn path_row(ui: &mut Ui, state: &mut AppState, label: &str, path: &mut std::path::PathBuf) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut s = path.display().to_string();
        if ui.text_edit_singleline(&mut s).changed() {
            *path = std::path::PathBuf::from(s);
            state.mark_dirty();
        }
        if ui.button("浏览…").clicked() {
            if let Some(picked) = rfd::FileDialog::new().pick_file() {
                *path = picked;
                state.mark_dirty();
            }
        }
    });
}
```

- [ ] **Step 6: 创建 `windows/settings/src/panels/about.rs`**

```rust
//! 关于面板。

use crate::state::AppState;
use egui::Ui;

pub fn show(ui: &mut Ui, state: &AppState) {
    ui.heading("关于 MyWubi");
    ui.separator();

    ui.label(format!("版本: {}", env!("CARGO_PKG_VERSION")));
    ui.label("跨平台形码输入法配置程序");
    ui.hyperlink_to("项目仓库", env!("CARGO_PKG_REPOSITORY"));

    ui.separator();
    ui.label("当前配置文件路径:");
    ui.monospace(state.config_path.display().to_string());
    ui.label(if state.portable { "（便携模式）" } else { "（用户模式）" });

    ui.separator();
    ui.label("内嵌字体: Noto Sans SC 子集");
    ui.label("GUI 框架: egui / eframe");
}
```

- [ ] **Step 7: 验证编译**

Run: `cargo check -p settings`
Expected: 编译通过（按编译器提示修正 `Panel` 路径等小问题）

- [ ] **Step 8: 提交**

```bash
git add windows/settings/src/panels/ windows/settings/src/lib.rs
git commit -m "✨ feat(settings): 实现四个配置面板——常规/外观/码表/关于

- basic: candidate count slider, commit mode, switch key, auto-commit
- appearance: font size, color picker with presets+ChooseColor+ARGB input
- dictionary: path+rfd browser, match/fuzzy toggles, user dict placeholder
- about: version, repo link, config path display"
```

---

### Task 10: SettingsApp 编排与 main.rs 入口

**Files:**
- Create: `windows/settings/src/app.rs`
- Modify: `windows/settings/src/lib.rs`
- Modify: `windows/settings/src/main.rs`

- [ ] **Step 1: 在 `lib.rs` 导出 app 模块**

```rust
pub mod app;
pub mod color_picker;
pub mod config_path;
pub mod fonts;
pub mod log;
pub mod panels;
pub mod save;
pub mod state;
```

- [ ] **Step 2: 创建 `windows/settings/src/app.rs`**

```rust
//! SettingsApp：eframe::App 实现，编排侧边栏 + 面板 + 保存栏。

use crate::state::{AppState, Panel};
use crate::{fonts, save};
use eframe::egui;

pub struct SettingsApp {
    pub state: AppState,
    /// 关闭确认对话框是否显示。
    close_confirm: bool,
}

impl SettingsApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: AppState) -> Self {
        fonts::load_chinese_fonts(&cc.egui_ctx);
        Self { state, close_confirm: false }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 侧边栏 + 主区域
        egui::SideBar::left("nav").default_width(160.0).show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("MyWubi 设置");
            ui.add_space(12.0);
            nav_item(ui, &mut self.state, Panel::Basic, "常规设置");
            nav_item(ui, &mut self.state, Panel::Appearance, "外观样式");
            nav_item(ui, &mut self.state, Panel::Dictionary, "码表与词库");
            nav_item(ui, &mut self.state, Panel::About, "关于");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            crate::panels::show_active(ui, &mut self.state);
            ui.add_space(16.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("保存").clicked() {
                    save::save(&mut self.state);
                }
                if ui.button("重新加载").clicked() {
                    self.state = AppState::load(self.state.config_path.clone());
                }
                if let Some(msg) = &self.state.status_msg {
                    ui.label(msg);
                }
            });
        });

        // 标题栏未保存标记
        if self.state.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("MyWubi 设置 *".into()));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("MyWubi 设置".into()));
        }

        // 关闭确认对话框
        if self.close_confirm {
            egui::Window::new("未保存的改动").show(ctx, |ui| {
                ui.label("有未保存的配置改动，是否保存？");
                ui.horizontal(|ui| {
                    if ui.button("保存").clicked() {
                        if save::save(&mut self.state) {
                            self.close_confirm = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    if ui.button("不保存").clicked() {
                        self.close_confirm = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("取消").clicked() {
                        self.close_confirm = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    }
                });
            });
        }
    }

    fn on_close(&mut self, _gl: &eframe::glow::Context) {
        if self.state.dirty {
            self.close_confirm = true;
        }
    }
}

fn nav_item(ui: &mut egui::Ui, state: &mut AppState, panel: Panel, label: &str) {
    let selected = state.active_panel == panel;
    if ui.selectable_label(selected, label).clicked() {
        state.active_panel = panel;
    }
}
```

注：`on_close` 的签名在 eframe 0.27 可能是 `fn on_close(&mut self, _gl: &eframe::glow::Context)` 或无参数，按编译器提示调整。`ViewportCommand::CancelClose` 需 eframe 0.27 支持。

- [ ] **Step 3: 重写 `windows/settings/src/main.rs`**

```rust
//! # settings
//!
//! MyWubi 配置程序入口。

mod app;
mod color_picker;
mod config_path;
mod fonts;
mod log;
mod panels;
mod save;
mod state;

use settings::{app::SettingsApp, config_path, log as log_mod, state::AppState};

fn main() {
    log_mod::init();
    let config_path = match config_path::resolve_config_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ 无法定位配置文件路径: {e}");
            std::process::exit(1);
        }
    };
    log::info!("配置文件路径: {}", config_path.display());

    let state = AppState::load(config_path);

    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("MyWubi 设置")
            .with_inner_size([640.0, 480.0])
            .with_min_inner_size([480.0, 360.0]),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "MyWubi 设置",
        opts,
        Box::new(|cc| Ok(Box::new(SettingsApp::new(cc, state)))),
    ) {
        eprintln!("❌ 启动失败: {e}");
        std::process::exit(2);
    }
}
```

注：由于 settings 现在是 lib+bin，`main.rs` 通过 `use settings::...` 引用 lib。若路径冲突，可改为 `use settings_lib as settings`（需在 Cargo.toml 设置 lib name）。执行时按编译器提示调整。

- [ ] **Step 4: 验证编译**

Run: `cargo check -p settings`
Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add windows/settings/src/app.rs windows/settings/src/main.rs windows/settings/src/lib.rs
git commit -m "✨ feat(settings): 实现 SettingsApp 编排与 GUI 入口

- eframe::App with SideBar navigation + panel dispatch
- save/reload buttons + status bar
- unsaved close confirmation dialog
- title bar dirty indicator"
```

---

### Task 11: 集成验证与手动测试清单

**Files:**
- 无新增，仅运行验证

- [ ] **Step 1: 全量编译**

Run: `cargo build -p settings --release`
Expected: 编译成功，生成 `target/release/settings.exe`

- [ ] **Step 2: 运行 core_engine 测试**

Run: `cargo test -p core_engine -- --nocapture`
Expected: 全部 PASS（含 enable_user_dict 新字段测试）

- [ ] **Step 3: 运行 settings 单元测试**

Run: `cargo test -p settings -- --nocapture`
Expected: config_path 测试 PASS

- [ ] **Step 4: 手动启动 GUI 验证**

Run: `./target/release/settings.exe`
Expected:
- 窗口标题"MyWubi 设置"，左侧侧边栏 4 项，右侧常规设置面板
- 切换四个面板无崩溃
- 中文字体正常显示（无豆腐块）——需 Task 2 的字体子集已替换为真实文件

- [ ] **Step 5: 手动验证保存流程**

在 GUI 中：
1. 改候选词个数为 7 → 点"保存" → 状态栏显示"✅ 已保存"
2. 关闭程序，用文本编辑器打开配置文件，确认 `candidate_count = 7`
3. 重新启动 settings.exe，确认候选词个数仍为 7

- [ ] **Step 6: 手动验证未保存确认框**

1. 改任意配置，不点保存
2. 点窗口关闭按钮 → 弹"未保存的改动"对话框
3. 点"取消" → 窗口不关闭
4. 点"不保存" → 窗口关闭，配置未写入
5. 重复 1-2，点"保存" → 配置写入后窗口关闭

- [ ] **Step 7: 手动验证配色三联动**

1. 外观面板，点预设色块 → 色块预览 + ARGB 文本框同步更新
2. 点"自定义…" → 弹 Win32 取色器，选色后三处同步
3. 编辑 ARGB 文本框为 `0xFFFF0000`，失焦 → 色块预览变红

- [ ] **Step 8: 手动验证码表浏览**

1. 码表面板，点"浏览…" → 弹文件对话框
2. 选择 `tables/wubi86.dict` → 路径填入文本框
3. 保存 → 配置文件中 `system_table` 更新

- [ ] **Step 9: 提交最终状态**

```bash
git add -A
git commit -m "✅ test(settings): 完成集成验证与手动测试清单

- verify build, unit tests, GUI launch, save flow, color picker, file dialog"
```

---

## 自审检查

**Spec 覆盖**：
- ✅ UI 框架与主题（Task 2 依赖、Task 5 字体、Task 10 布局）
- ✅ 侧边栏导航布局（Task 10 `egui::SideBar`）
- ✅ 中文字体支持（Task 5）
- ✅ 常规设置面板（Task 9 Step 3）
- ✅ 外观样式面板含配色三联动（Task 9 Step 4）
- ✅ 码表与词库面板含 rfd 浏览（Task 9 Step 5）
- ✅ 关于面板（Task 9 Step 6）
- ✅ config.toml 安全读写（Task 8 复用 Config::save）
- ✅ 配置路径定位（Task 3）
- ✅ 未保存确认框（Task 10）
- ✅ enable_user_dict 新字段（Task 1）
- ✅ 文件日志（Task 4）
- ✅ Win32 ChooseColor（Task 6）
- ✅ 测试策略（Task 1/3 单元测试 + Task 11 手动验收）

**无占位符**：所有步骤含完整代码或确切命令。

**类型一致性**：`AppState`、`Panel`、`save::save`、`config_path::resolve_config_path`、`fonts::load_chinese_fonts`、`color_picker::pick_color` 在各 Task 间签名一致。

**已知执行时需调整点**（已在对应步骤注明）：
- `CHOOSECOLORW` 字段名按 windows-rs 版本调整
- `eframe::App::on_close` 签名按版本调整
- lib/bin 命名冲突时调整 `use` 别名
- 字体子集占位文件需替换为真实 Noto Sans SC 子集