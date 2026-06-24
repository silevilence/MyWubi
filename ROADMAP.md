# 项目开发路线图 (Roadmap)

## 📅 计划中

- [ ] **配置界面基础 UI 框架与主题搭建**
    - [ ] 引入 `egui` 和 `eframe` 依赖
    - [ ] 设计配置界面的整体网格布局（侧边栏导航 + 右侧主配置区域）
    - [ ] 配置中文字体支持（集成开源中文字体，防止出现豆腐块乱码）

- [ ] **配置表单与交互开发**
    - [ ] 实现“常规设置”面板
        - [ ] 候选词个数选择（Slider 组件）
        - [ ] 常用快捷键设置（Combobox 如下屏方式、中英文切换键）
    - [ ] 实现“外观样式”面板
        - [ ] 候选框字体大小、皮肤配色调整（ColorPicker 组件）
    - [ ] 实现“码表与词库”面板
        - [ ] 码表文件路径选择（集成本地文件选择对话框）
        - [ ] 词频调整与用户自造词导出/导入

- [ ] **配置持久化与同步**
    - [ ] 实现 `config.toml` 的读取、修改与安全写入逻辑（防写入中断损坏文件）
    - [ ] 编写配置变更保存时的通知机制，确保输入法后台即时生效

- [ ] **DLL 注册与反注册脚本及 Hook 开发**
    - [ ] 编写 `reg_script`（基于 `regsvr32` 或直接操作注册表注册 TSF 类 ID）
    - [ ] 编写 Rust 注册逻辑，并在 DLL 导出 `DllRegisterServer` 和 `DllUnregisterServer`
    - [ ] **在设置工具中集成 TIP 注册/启用功能**
        - [ ] 通过 `ITfInputProcessorProfileMgr` COM 接口实现 TIP 的注册与启用（替代注册表方案，绕过未签名 DLL 的"仅桌面"灰显限制）
        - [ ] 实现输入法的安装、启用、禁用、卸载全生命周期管理
        - [ ] 设置工具启动时检测 TIP 注册状态并提示用户操作

- [ ] **Velopack 打包配置与自动化构建**
    - [ ] 安装并配置 `vshere` 和 Velopack CLI 工具
    - [ ] 编写构建脚本（如 `build.rs` 或 `powershell` 脚本）
        - [ ] 编译 `im_engine.dll` (release)
        - [ ] 编译 `settings.exe` (release)
        - [ ] 收集相关资源文件（默认码表、字体、默认配置文件）
    - [ ] 配置 Velopack 安装/卸载 Hook
        - [ ] **安装/更新 Hook**：自动调用注册程序将 `im_engine.dll` 注册到系统 TSF
        - [ ] **卸载 Hook**：解除 TSF 占用，注销 DLL，清理注册表
- [ ] **一键打包与发布验证**
    - [ ] 生成最终安装包 `Setup.exe`
    - [ ] 在干净的 Windows 虚机中测试安装、激活、打字、配置修改、自动更新、卸载全流程

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
