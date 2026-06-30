# 项目开发路线图 (Roadmap)

## 📅 计划中

- [ ] **TSF 语言栏按钮** (2026-06-27-tsf-full-integration-plan Phase 2.4)
    - [ ] 实现 `ITfLangBarItemButton` 中/英状态指示按钮
    - [ ] 通过 `ITfLangBarItemMgr::AddItem` 注册
    - [ ] 绑定到 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` 自动反映状态
    - [ ] 按钮点击触发 `toggle_ime_mode()`

- [ ] **Rust 核心 JNI 桥接层设计 (core_engine)**
    - [ ] 引入 `jni` crate 依赖
    - [ ] 导出适配 Android 的 C-ABI 接口
        - [ ] 实现 `Java_com_example_inputmethod_CoreEngine_init` 接口
        - [ ] 实现 `Java_com_example_inputmethod_CoreEngine_search` 检索接口
        - [ ] 实现 `Java_com_example_inputmethod_CoreEngine_onKeyEvent` 按键处理接口

- [ ] **Android 原生输入法服务开发 (Kotlin)**
    - [ ] 搭建 Android Studio 工程，配置多架构 Rust 交叉编译
    - [ ] 继承 `InputMethodService` 实现输入法生命周期
    - [ ] 编写 JNI 载入器，加载编译出的 `.so` 核心动态库

- [ ] **Android 候选框与设置界面开发 (Jetpack Compose)**
    - [ ] 使用 **Jetpack Compose** 绘制输入法候选栏 UI
    - [ ] 实现按键振动反馈、候选词滑动翻页等移动端专属交互
    - [ ] 使用 Compose 构建轻量化配置界面，将配置同步至 Android SharedPreferences 或直接写入配置文件

- [ ] **自动化构建集成**
    - [ ] 配置 `rust-android-gradle` 插件
    - [ ] 实现通过 Gradle 一键编译 Rust 底层、拷贝 `.so` 到相应 ABI 目录并打包输出 `.apk`

- [ ] **im_engine 配置路径定位同步**
    - [ ] `im_engine.dll` 的 `notify` 监听路径需采用与 `settings.exe` 相同的 `resolve_config_path` 定位逻辑（exe 同目录优先，回退 `%APPDATA%\MyWubi\`）
    - [ ] 确保双进程读写同一份 `config.toml`，避免便携模式与用户模式路径不一致导致热重载失效

- [ ] **用户词库功能（core_engine + UI 联调）**
    - [ ] `core_engine` 设计用户词库数据结构（独立于 `config.toml` 的存储格式）
    - [ ] `core_engine` 暴露用户词库 CRUD / 导入导出 C-ABI 接口
    - [ ] `settings.exe` 码表面板"管理自造词…"按钮对接真实功能（替换"待开发"提示）
    - [ ] `im_engine` 侧用户词库的检索集成与热重载

## 🚧 开发中

## ✅ 已完成

- [x] **项目工作空间与多模块骨架搭建**
    - [x] 初始化 Cargo Workspace 根目录
    - [x] 创建 `core_engine` 库项目 (Rust library)
    - [x] 创建 `im_engine` 动态库项目 (Rust cdylib)
    - [x] 创建 `settings` 二进制项目 (Rust binary)

- [x] **核心数据结构与码表解析设计**
    - [x] 设计 `config.toml` 配置解析器（基于 `serde` 和 `toml`）
    - [x] 设计码表文件格式及高效内存映射结构
        - [x] 实现针对形码检索优化的数据结构（有序数组二分查找 + 前缀 Trie）
        - [x] 支持大码表文件的分块加载或懒加载机制

- [x] **输入法状态机与检索算法实现**
    - [x] 实现输入缓冲区管理（Spelling Buffer，处理用户当前输入的编码）
    - [x] 实现核心检索匹配逻辑（精确匹配、前缀匹配、多词频排序规则）
    - [x] 实现输入状态转换机（处理上屏、退格、清空、翻页等状态）

- [x] **自动化测试与质量保障**
    - [x] 编写码表解析器单元测试（验证边界条件、非法字符处理）
    - [x] 编写状态机集成测试（模拟经典输入流，验证输出上屏词条是否符合预期）
    - [x] 配置 Benchmarks 性能基准测试（确保单次检索延迟控制在微秒级）

- [x] **TSF 接口对接与 COM 注册**
    - [x] 使用 `windows-rs` 声明 TSF 所需的核心 COM 接口
    - [x] 实现输入法生命周期接口
        - [x] 实现 `ITfTextInputProcessor` 接口（激活与去激活）
        - [x] 实现 `ITfThreadMgrEventSink` 接口（监听焦点切换）
    - [x] 实现按键过滤与拦截
        - [x] 实现 `ITfKeyEventSink` 接口
        - [x] 编写按键拦截规则（字母键入缓冲区，空格/数字键选择候选，Esc 清空）
    - [x] 编写 `reg_script`（基于 `regsvr32` 或直接操作注册表注册 TSF 类 ID）
        - [x] PowerShell 脱离 regsvr32 的备用注册脚本 `windows/im_engine/reg_script.ps1`
    - [x] 编写 Rust 注册逻辑，并在 DLL 导出 `DllRegisterServer` 和 `DllUnregisterServer`
        - [x] 同时导出 `DllGetClassObject` / `DllCanUnloadNow` / `DllMain`

- [x] **基于 Slint 的轻量化候选框 UI 绘制**
    - [x] 编写 `candidate_window.slint` 界面定义文件（极致精简、无边框、支持主题色）
    - [x] 在 `im_engine` 中集成 Slint 编译器
    - [x] 实现 TSF 窗口与 Slint 渲染窗口的绑定
        - [x] 获取当前排版引擎的光标位置 (IPoint / `ITfContext::GetStatus`)
        - [x] 实现候选框窗口随光标绝对定位定位与自动避让屏幕边缘
        - [x] 实现候选框的无焦点（No-Focus）弹出，避免夺取主应用焦点

- [x] **数据通道与热重载**
    - [x] 实现本地 `config.toml` 的变更监听（基于 `notify` 库）
    - [x] 实现内存中配置对象的原子更新（使用 `ArcSwap` 或读写锁）

- [x] **配置界面基础 UI 框架与主题搭建**
    - [x] 引入 `egui` 和 `eframe` 依赖
    - [x] 设计配置界面的整体网格布局（侧边栏导航 + 右侧主配置区域）
    - [x] 配置中文字体支持（集成开源中文字体，防止出现豆腐块乱码）

- [x] **配置表单与交互开发**
    - [x] 实现“常规设置”面板
        - [x] 候选词个数选择（Slider 组件）
        - [x] 常用快捷键设置（Combobox 如下屏方式、中英文切换键）
    - [x] 实现“外观样式”面板
        - [x] 候选框字体大小、皮肤配色调整（ColorPicker 组件）
    - [x] 实现“码表与词库”面板
        - [x] 码表文件路径选择（集成本地文件选择对话框）
        - [x] 词频调整与用户自造词导出/导入

- [x] **配置持久化与同步**
    - [x] 实现 `config.toml` 的读取、修改与安全写入逻辑（防写入中断损坏文件）
    - [x] 编写配置变更保存时的通知机制，确保输入法后台即时生效

- [x] **DLL 注册与反注册脚本及 Hook 开发**
    - [x] 编写 `reg_script`（基于 `regsvr32` 或直接操作注册表注册 TSF 类 ID）
    - [x] 编写 Rust 注册逻辑，并在 DLL 导出 `DllRegisterServer` 和 `DllUnregisterServer`
    - [x] **在设置工具中集成 TIP 注册/启用功能**
        - [x] 通过 `ITfInputProcessorProfileMgr` COM 接口实现 TIP 的注册与启用（替代注册表方案，绕过未签名 DLL 的"仅桌面"灰显限制）
        - [x] 实现输入法的安装、启用、禁用、卸载全生命周期管理
        - [x] 设置工具启动时检测 TIP 注册状态并提示用户操作

- [x] **废弃旧注册脚本**
    - [x] 所有独立安装脚本已移除（`register_tip.bat`、`enable_tip.ps1`、`reg_script.ps1` 等），统一通过 `settings.exe` 中的「输入法管理」面板安装

- [x] **windows-rs 版本升级 (0.61 → 0.62+)**
    > 当前 0.61 缺失以下关键 TSF API 方法，需要升级后才能使用
    - [x] **评估变更范围**：`#[implement]` 宏 + `ComObjectInner` trait + `Interface` 绑定契约均已断裂
    - [x] **升级后启用的功能**：
        - `ITfCategoryMgr::RegisterGUID()` — 获取 TfGuidAtom，在 composition range 上设置 `GUID_PROP_ATTRIBUTE` 显示编码/候选态下划线
        - `IEnumTfDisplayAttributeInfo_Impl` trait — 完整实现 `ITfDisplayAttributeProvider::EnumDisplayAttributeInfo`（当前返回 E_FAIL）
    - [x] **迁移步骤**：
        1. 更新 `Cargo.toml` 中的 workspace 依赖 `windows = "0.62"`
        2. 适配 `#[implement]` 宏的 `IUnknownImpl` + `ComObjectInner` 新契约
        3. 验证所有 COM 对象的 `QueryInterface` 和引用计数正确性
        4. 恢复 `IEnumTfDisplayAttributeInfo` 枚举器实现
        5. 在 `edit_session_composition_update` 中设置 DisplayAttr

- [x] **GitHub Actions 自动化流水线构建**
    - [x] 触发机制：监控指定格式的版本标签（大小写不敏感，如 `v*.*.*` 或 `V*.*.*`），自动激活发布工作流
    - [x] 工作流步骤：
        - [x] 变更日志解析与健全性校验：读取 `CHANGELOG.md` 中对应版本号的发布说明，若解析失败（版本号不匹配或格式异常）则触发异常中断。格式约定如下：
            ```markdown
            # Change Log

            ## V0.1.0

            更新内容（一整个二级标题下均视为更新内容）
            ```
            注：版本号倒序排列（新版本在前），检索时需进行大小写不敏感匹配。
        - [x] 核心产物编译：执行 `cargo build --release` 编译核心二进制程序
        - [x] 资源归档：通过 `package.ps1` 收集构建产物及必要依赖，并压缩为 Zip 便携包
        - [x] 资产发布：创建 GitHub Release，将解析出的更新说明填入 Release Body，并上传 Zip 归档文件

- [x] **Velopack 分发包装配与自动化构建**
    - [x] **运行分发环境与更新源配置**
        - [x] 初始化并部署 Velopack CLI 编译工具链环境
        - [x] 配置 GitHub Release（`silevilence/MyWubi`）作为官方增量更新（Delta Updates）的核心分发源
        - [x] **按需更新触发机制**：于 `settings.exe`“关于”面板集成检查更新交互控件，实现用户自主唤醒的按需（On-demand）更新轮询
        - [x] **运行时更新策略决策路由**：构建宿主环境感知模块，动态判别当前软件执行模式。绿色便携版运行模式下，采用“用户确认后重定向至发布页自主下载”的被动更新策略；标准安装版运行模式下，则调用 Velopack 客户端 API 执行静默/热增量更新机制
    - [x] **多项目自动化编译与静态资源整合**
        - [x] 编写全自动构建与装配脚本（如 `build.rs` 或 PowerShell 自动化脚本）
        - [x] 编译核心输入法引擎：`im_engine.dll` (Release 优化模式)
        - [x] 编译独立配置管理器：`settings.exe` (Release 优化模式)
        - [x] 静态资产打包整合：内置全局默认码表、字体文件以及核心基础配置文件 `config.toml` 的自动化封装
    - [x] **系统服务级生命周期钩子（Lifecycle Hooks）控制**
        - [x] **安装与更新阶段**：调用 `tip_manager::install()` 实现文本服务框架（TSF）的自动注册
        - [x] **卸载阶段**：调用 `tip_manager::uninstall()` 实现 TSF 服务的反注册及残留清理
    - [x] **多渠道分发验证与端到端闭环测试**
        - [x] 构建标准安装程序 `Setup.exe`
        - [x] 在干净的 Windows 虚拟化测试环境中，进行安装、激活、输入法调度、配置更新、静默升级、卸载全流程的闭环验证
    - [x] **CI/CD 流水线集成与双通道发布**
        - [x] 在 GitHub Actions 中废弃原有打包脚本，全面接入 Velopack 引擎
        - [x] 自动化流水线一键生成并发布绿色便携版（Zip）与标准安装包（`Setup.exe`）双渠道资产

- [x] **翻页热键可配置**
    - [x] `hotkey.page_next` / `hotkey.page_prev` 配置项实际接到按键层
    - [x] `settings.exe` 常规面板新增翻页键选择
