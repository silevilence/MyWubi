# AGENTS.md - 跨平台形码输入法 Agent 开发指导说明书

为了保证不同 AI Agent 或开发人员在协同开发该项目时，能够维持统一的架构设计、技术选型及编码规范，特制定本开发说明书。

---

## 1. 总体架构与选型共识

本项目是一个**跨平台形码（如五笔、郑码、自定义形码）输入法**。其核心设计思想是**“核心算法跨平台、前端壳层彻底解耦、Windows端双进程隔离”**。

```
                       +-------------------+
                       |    Core Engine    |  <-- 纯 Rust 核心算法层
                       | (码表/检索/状态机)  |  (C-ABI / JNI 暴露)
                       +---------+---------+
                                 |
                +----------------+----------------+
                |                                 |
                v                                 v
      +---------------------------+     +-------------------+
      |   Windows Frontend        |     |  Android Frontend |
      | (双进程,物理隔离)         |     | (Kotlin+Compose)  |
      +--------+------------------+     +---------+---------+
               |           |                       |
       +-------+---+  +----+--------+              v
       |           |  |             |     JNI 调用 Core Engine
       v           v  v             v         实现本地快速检索
 im_engine.dll  tip_manager  settings.exe
  (TSF输入法)   (TIP生命周期)  (egui配置程序)
```

### 1.1 技术栈矩阵

| 模块 | 语言 / 框架 | 关键说明 |
| :--- | :--- | :--- |
| **核心算法层 (Core)** | Rust (No-std 友好) | 负责码表解析、Trie树检索、拼音/形码混合状态机、配置文件读写。不依赖任何 UI 和系统平台 API。 |
| **Windows 输入法本体** | Rust + `windows-rs` (TSF) + **GDI** | 编译为 `im_engine.dll`。负责接管系统输入事件、计算光标位置。候选框采用 **GDI `UpdateLayeredWindow`** 透明分层窗口渲染。 |
| **Windows 配置程序** | Rust + **egui / eframe** | 编译为 `settings.exe`。常规的配置修改程序。采用 **egui** 框架，无复杂的 C++ 编译链依赖，开箱即用，包体极小，UI 开发简单直接。 |
| **Windows TIP 管理器** | Rust + `windows-rs` COM | 编译为静态库 `tip_manager`，被 `im_engine.dll` 和 `settings.exe` 共用。封装 TIP 全生命周期（注册/安装/启用/禁用/卸载）。 |
| **Android 端外壳** | Kotlin + Jetpack Compose | 继承系统的 `InputMethodService`，实现标准的输入法服务。 |
| **Android 核心调用** | JNI + Rust 核心静态库 | 通过 JNI 将 Rust 核心编进 Android `.so`，并在 Kotlin 层直接调用。 |
| **Windows 打包发布** | **Velopack**（`build.ps1`） | 通过 `build.ps1` 一键编译 Release 产物并调用 `vpk pack` 生成安装包（Setup.exe）、便携包（Portable.zip）和增量更新包。CI 通过 `.github/workflows/release.yml` 在推送版本标签时自动触发。 |

---

## 2. 目录结构规范

项目采用 Cargo Workspace 进行多项目管理。

```
MyWubi/
├── Cargo.toml                  # Workspace 配置文件
├── AGENTS.md                   # 本开发规范文档
├── config.toml                 # 输入法全局配置文件（双端通用格式）
├── build.ps1                   # Velopack 打包脚本（主要）
├── package.ps1                 # 简易打包脚本（拷贝到 deploy/）
│
├── .github/workflows/
│   └── release.yml             # CI 发布流水线（Velopack）
│
├── core_engine/                # 核心算法层 (库项目)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs              # 导出 C-ABI 接口与 Rust 内部接口
│   │   ├── dictionary.rs       # 码表解析与检索 (Trie 树/二分查找)
│   │   ├── state_machine.rs    # 输入状态机（处理上屏、清码、候选词切换）
│   │   └── config.rs           # 配置文件解析 (toml-rs)
│   ├── tests/                  # 集成测试（状态机端到端流程）
│   └── benches/                # 性能基准测试 (criterion)
│
├── windows/                    # Windows 平台前端
│   ├── im_engine/              # 输入法本体 DLL
│   │   ├── Cargo.toml
│   │   ├── build.rs            # 构建脚本（当前为空：候选框已是纯 GDI）
│   │   └── src/
│   │       ├── lib.rs          # COM 服务器入口 (DllMain/DllGetClassObject)
│   │       ├── text_service.rs # TSF 全部 COM 接口实现
│   │       ├── candidate_window.rs # GDI 透明分层候选框窗口
│   │       ├── factory.rs      # IClassFactory 实现
│   │       ├── key_filter.rs   # 虚拟键码→InputEvent 映射
│   │       ├── screen_geometry.rs # 光标定位与屏幕避让
│   │       ├── candidate_data.rs   # 候选框共享数据结构
│   │       ├── file_log.rs     # 文件日志输出
│   │       └── guids.rs        # GUID 常量
│   │
│   ├── settings/               # 独立配置程序 exe
│   │   ├── Cargo.toml
│   │   ├── build.rs            # 嵌入 requireAdministrator 清单 + 码表复制
│   │   ├── assets/tables/      # 构建时自动复制到输出的码表模板
│   │   └── src/
│   │       ├── main.rs         # 入口（管理员检查 + COM 初始化）
│   │       ├── lib.rs          # 模块导出
│   │       ├── app.rs          # SettingsApp 编排
│   │       ├── state.rs        # AppState 状态容器
│   │       ├── save.rs         # 原子写入保存
│   │       ├── config_path.rs  # 配置路径定位（便携优先，回退 AppData）
│   │       ├── vpk.rs          # Velopack 按需更新检查
│   │       ├── log.rs          # 日志初始化
│   │       ├── elevation.rs    # 管理员权限检测与提权
│   │       ├── panels/         # 五个配置面板（常规/外观/码表/输入法管理/关于）
│   │       ├── color_picker.rs # Win32 ChooseColor 原生颜色对话框
│   │       └── fonts.rs        # 内嵌 Noto Sans SC 字体管理
│   │
│   └── tip_manager/            # TIP 生命周期管理库
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs          # install/uninstall/enable/disable 入口
│           ├── profile.rs      # ITfInputProcessorProfileMgr COM 封装
│           ├── registrar.rs    # 注册表写入与删除
│           ├── detect.rs       # 安装状态检测
│           └── guids.rs        # CLSID / Profile GUID 常量
│
├── tables/                     # 码表目录
│   └── wubi86.dict             # 五笔 86 版码表
├── Releases/                   # Velopack 打包产物目录
├── deploy/                     # 简易打包输出目录
├── docs/                       # 设计文档与计划
└── CHANGELOG.md                # 版本发布日志
```

---

## 3. 核心模块开发指南

### 3.1 核心算法层 (core)
*   **无状态与高并发安全**：核心检索引擎应当尽量保持只读。码表载入内存后（如使用 `OnceCell` 或 `Lazy`），多线程并发检索不应有写锁冲突。
*   **检索算法**：
    *   必须支持高并发的**二分查找**或前缀**Trie 树**检索。
    *   对重码率高的词条需支持“词频权重”排序。
*   **C-ABI 接口暴露**：
    *   必须使用 `#[no_mangle]` 导出 C 兼容接口，以便 Android JNI 和 Windows TSF 的 C++（若有）/ Rust 边界互相调用。
    *   所有指针传递必须有严格的生存期管理，提供明确的 `destroy` 函数释放 Rust 侧持有的内存。

### 3.2 Windows 独立配置程序 (settings) - egui/eframe
*   **定位**：常规桌面 GUI 应用程序，用于对码表、外观、快捷键、用户词典进行可视化配置。
*   **技术选型**：**egui + eframe**。
    *   *为什么选择 egui*：它是目前 Rust 生态中**最成熟、最常规、社区最活跃**的 GUI 框架之一。采用即时渲染（Immediate Mode），没有复杂的生命周期和所有权心智负担，界面开箱即用，自带大量常规表单控件（Slider, Checkbox, ComboBox, TextEdit 等），非常适合写配置面板这种"表单堆砌"型界面。同时它是纯 Rust 像素级渲染（基于 wgpu/glow），不需要配置复杂的 C++ 编译链。
*   **核心逻辑**：
    1. 启动时检查管理员权限（必需，否则弹窗退出）。
    2. 初始化 COM（`CoInitializeEx`），供 `tip_manager` 调用。
    3. 通过 `config_path::resolve_config_path()` 定位配置目录（便携模式优先，回退 `%APPDATA%\MyWubi\`）。
    4. 使用 `egui` 提供直观的 Tab 页切换（常规设置、外观设置、码表管理、输入法管理、关于）。
    5. 点击"保存"时，通过 `Config::save` 原子写回 `config.toml`。
    6. 关于面板通过 `vpk` 模块支持「检查更新」功能，调用 Velopack API 查询新版本。

### 3.3 Windows 输入法本体 (im_engine) - TSF + GDI
*   **定位**：无焦点、超轻量、常驻后台的系统级 DLL。
*   **技术选型**：`windows-rs` (TSF) + **GDI**。
    *   *候选框渲染*：采用纯 Win32 GDI `UpdateLayeredWindow` 透明分层窗口，独立线程 + 16ms 定时器轮询刷新，通过 `ArcSwap` 无锁读取候选数据。**不要在 DLL 里初始化大型 GUI 运行环境**。
*   **核心逻辑**：
    *   通过 TSF 框架接管系统输入事件，`ITfKeyEventSink::OnKeyDown` 将虚拟键码通过 `key_filter` 模块翻译为 [`InputEvent`]（翻页键按 `config.toml` 中 `[hotkey]` 的 `page_next`/`page_prev` 配置动态映射），驱动 [`StateMachine`] 并依据 [`Transition`] 决定是否拦截按键。
    *   TSF 激活时通过 `ITfTextInputProcessorEx::Activate` 保存线程管理器，注册 `ITfKeyEventSink`、`ITfThreadMgrEventSink`、`ITfThreadFocusSink`、`ITfTextEditSink` 等事件接收器。
    *   候选框通过 `ArcSwap` 无锁原子替换共享 [`CandidateData`]，渲染线程每 16ms 读取一次最新数据绘制到 GDI 分层窗口。
    *   配置热重载通过 `ArcSwap` 原子替换实现（无需 `notify` 文件监听，由 TSF 事件或定时器触发重新加载）。

### 3.4 TIP 管理器 (tip_manager)
*   **定位**：被 `im_engine.dll` 和 `settings.exe` 共同链接的静态库，封装 TSF TIP 全生命周期操作。
*   **技术选型**：`windows-rs` COM + 注册表 API。
*   **核心逻辑**：
    *   **注册表操作**：`registrar::register_tip` 在 `HKLM\SOFTWARE\Classes\CLSID\` 和 `HKLM\SOFTWARE\Microsoft\CTF\TIP\` 下写入 TIP 注册信息。
    *   **COM Profile 管理**：通过 `ITfInputProcessorProfileMgr` 调用 `RegisterProfile` / `UnregisterProfile` / `Enable` / `Disable`。
    *   **类别注册**：通过 `ITfCategoryMgr` 注册 `GUID_TFCAT_TIP_KEYBOARD` 等 TSF 类别，替代手动注册表写入。
    *   **TIP 状态检测**：`detect` 模块检测当前 TIP 的安装/启用状态（通过注册表 + COM 查询）。

### 3.5 Android 壳层 (android)
*   **JNI 调用约束**：
    *   不要在 Kotlin 频繁的打字事件中重复创建 Rust 对象。应在 `InputMethodService` 启动时，初始化 Rust 的全局检索指针，并在销毁时显式释放。
    *   JNI 传递字符串时注意 UTF-8 与 JVM 内部 UTF-16 的转换开销。
*   **自动化构建集成**：
    *   使用 `rust-android-gradle` 插件，在执行 `./gradlew assembleDebug` 时自动触发 `cargo build --target <arch>` 并将生成的 `.so` 自动放入 Android 项目的 `jniLibs` 中。

---

## 4. 协同开发与代码提交规范

1.  **AI Agent 修改边界**：
    *   若修改核心算法，严禁混入任何平台专属（Windows/Android）的系统调用。
    *   UI 的改动应分别在 `windows/settings` (egui)、`windows/im_engine` (GDI) 或 `android` (Compose) 中进行。
2.  **配置文件一致性**：
    *   修改 `config.toml` 格式时，必须同时更新 `core/src/config.rs` 的反序列化结构体，以及双端（egui 和 Compose）对应的配置界面。
3.  **日志规范**：
    *   严禁在 Windows TSF 核心中使用 `println!`。由于是 DLL 注入运行，使用 `log` 库并重定向到本地隐藏文件夹下的 `debug.log`。
4. **文档修改禁令**：
   非用户允许，禁止主动修改 `ROADMAP.md` 与 `AGENTS.md`

### Git提交规范

一般不建议主动提交git，必须提交时，git消息需遵循以下规范

#### 格式

```
<emoji> <type>(<scope>): <subject>

<body>

<footer>
```

- **emoji**：视觉分类标识，必须使用
- **type**：`feat` / `fix` / `refactor` / `docs` / `test` / `chore` / `style` / `perf`
- **scope**：可选，如 `(opds)`、`(spider)`、`(api)`、`(web)`
- **subject**：中文标题，概括变更内容，首字无需空格
- **body**：英文或中英文混排，每行为一个 `- ` 开头的条目，描述具体变更
- **footer**：可选的 `Refs:` 或 `BREAKING CHANGE:`

#### Emoji 对照表

| Type | Emoji | 含义 |
|---|---|---|
| `feat` | ✨ | 新功能 |
| `fix` | 🐛 | Bug 修复 |
| `refactor` | ♻️ | 代码重构 |
| `docs` | 📚 | 文档变更 |
| `test` | 🧪 | 测试相关 |
| `chore` | 🔧 | 工程化/依赖/配置 |
| `style` | 🎨 | 代码格式/样式 |
| `perf` | ⚡ | 性能优化 |
| `wip` | 🚧 | 进行中（仅临时使用，合并前必须 squash） |

#### 示例

```
✨ feat(opds): 实现 OPDS 基础层——可见性控制与 EPUB 制品生命周期

- DB: add opds_visible, content_updated_at, epub_compiled_at columns
- Repository: add OPDS CRUD methods
- OpdsCompilationService: new cron-based scheduler

Refs: ROADMAP OPDS 书源服务构建与分发
```

```
🐛 fix(api): 修复定时更新策略变更后调度器未正确重载的并发问题
```

```
📚 docs: 添加 OPDS 书源服务任务到路线图
```

#### 约定

- 多条变更在同一提交中时，`subject` 概括主要变更，`body` 逐条列举
- 每行 body 以 `- ` 开头，长度不超过 72 字符（英文）或适当截断
- **禁止**仅重复文件列表而无语义描述的提交
- **禁止**在提交消息中包含内部指令或占位符（如 "TODO"、"TBD"）

## 5. 依赖管理

所有依赖管理，尽可能使用工具实现（如 `cargo add`）。

## Agent skills

### Issue tracker

Issues are tracked as local markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles use default label names. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout. See `docs/agents/domain.md`.