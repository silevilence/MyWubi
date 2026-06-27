# MyWubi — 跨平台形码输入法

MyWubi 是一个跨平台的形码（五笔、郑码、自定义形码）输入法引擎，目前提供 **Windows 版本**（基于 TSF 框架），Android 版本正在开发中。

## 功能亮点

- **TSF 输入法引擎** — 基于 Windows Text Services Framework，支持完整的文本上屏流水线：组合编辑（Composition）、编辑会话（EditSession）、显示属性、中/英模式切换、按键透传与拦截
- **候选窗口** — 使用 GDI 渲染的透明分层窗口，支持字体样式、屏幕避让定位、16ms 定时器轮询刷新
- **设置程序** — 基于 egui/eframe 的图形化配置界面，提供常规、外观、码表和关于四个面板
- **码表管理** — 支持码表目录化浏览选择、初始化导入和预览高亮
- **外观定制** — 可自定义候选框颜色（使用 Win32 原生颜色对话框）、内嵌 Noto Sans SC 字体
- **便携模式** — 自动检测便携模式（优先使用本地目录），否则回退到 AppData
- **TIP 生命周期管理** — 支持输入法的安装、启用、禁用、卸载全流程
- **配置热重载** — 通过 `ArcSwap` 无锁原子替换，修改 `config.toml` 后无需重启
- **文件日志** — 设置程序和输入法引擎分别输出日志到 `log/` 和 `%LOCALAPPDATA%\MyWubi\`，便于排查问题

## 系统要求

- **操作系统**：Windows 10/11（64 位）
- **Rust 编译器**：1.75+（如自行编译）
- **可选**：中文语言包（简体中文，Windows 设置中安装）

## 快速开始

### 下载使用

从 [Releases](https://github.com/your-org/MyWubi/releases) 下载最新版本的 `deploy.zip`，解压后：

```powershell
# 1. 以管理员身份运行注册脚本
.\register_tip.bat

# 2. 以管理员身份运行启用脚本
.\enable_tip.ps1

# 3. 重启输入法服务
taskkill /f /im ctfmon.exe
# ctfmon 会自动重启

# 4. 按 Win+Space 切换到 MyWubi 即可使用
```

> 也可以通过 `settings.exe` 中的「输入法管理」面板一键安装/启用。

### 从源码编译

```powershell
# 编译全部（Debug）
cargo build

# 编译全部（Release）
cargo build --release
```

### 打包部署

```powershell
# Debug 构建打包
.\package.ps1

# Release 构建打包
.\package.ps1 -Release
```

打包产物位于 `deploy/` 目录。

## 运行设置程序

```powershell
# 必须以管理员身份运行
.\deploy\settings.exe
```

## 测试

```powershell
# 运行核心引擎测试
cargo test -p core_engine

# 运行所有测试
cargo test

# 运行基准测试
cargo bench -p core_engine --bench dictionary_bench
```

## 配置文件

`config.toml` 位于程序所在目录（便携模式）或 `%APPDATA%\MyWubi\`（用户模式）：

- **基础设置**：候选个数、上屏方式、中英文切换键、四码自动上屏
- **外观设置**：字体大小、主色、背景色、高亮色
- **码表设置**：系统码表路径、用户词库路径
- **快捷键**：翻页键、简码切换

## 项目结构

```
MyWubi/
├── Cargo.toml                  # 工作空间配置
├── config.toml                 # 全局配置文件
├── package.ps1                 # 打包脚本
│
├── core_engine/                # 核心算法层（纯 Rust）
│   ├── src/
│   │   ├── lib.rs              # C-ABI 导出与公共 API
│   │   ├── config.rs           # 配置解析（serde + toml）
│   │   ├── dictionary.rs       # 码表解析与 Trie/二分检索
│   │   └── state_machine.rs    # 输入状态机
│   ├── tests/                  # 集成测试
│   └── benches/                # 性能基准
│
├── windows/
│   ├── im_engine/              # TSF 输入法本体 DLL
│   │   ├── src/
│   │   │   ├── lib.rs          # COM 服务器入口 & 引擎单例
│   │   │   ├── text_service.rs # ITfTextInputProcessor 实现
│   │   │   ├── candidate_window.rs  # GDI 透明候选框
│   │   │   ├── factory.rs      # IClassFactory 实现
│   │   │   ├── key_filter.rs   # 虚拟键码→InputEvent 映射
│   │   │   ├── screen_geometry.rs # 光标定位与屏幕避让
│   │   │   ├── candidate_data.rs   # 候选框共享数据结构
│   │   │   ├── file_log.rs     # 文件日志
│   │   │   └── guids.rs        # GUID 常量
│   │   └── register_tip.bat    # 注册脚本
│   │
│   ├── settings/               # 配置程序 exe（egui/eframe）
│   │   ├── src/
│   │   │   ├── main.rs         # 入口（管理员检查 + COM 初始化）
│   │   │   ├── lib.rs          # 模块导出
│   │   │   ├── app.rs          # SettingsApp 编排
│   │   │   ├── panels/         # 四个配置面板
│   │   │   ├── state.rs        # 状态容器
│   │   │   ├── config_path.rs  # 路径定位逻辑
│   │   │   ├── save.rs         # 原子写入保存
│   │   │   ├── color_picker.rs # Win32 ChooseColor 封装
│   │   │   └── fonts.rs        # 字体管理
│   │   └── build.rs            # 管理员清单嵌入 + 码表复制
│   │
│   └── tip_manager/            # TIP 生命周期管理库
│       └── src/
│           ├── lib.rs          # install/uninstall/enable/disable
│           ├── profile.rs      # ITfInputProcessorProfileMgr 封装
│           ├── registrar.rs    # 注册表写入与删除
│           ├── detect.rs       # 状态检测
│           └── guids.rs        # GUID 常量
│
├── tables/                     # 码表目录
│   └── wubi86.dict             # 五笔 86 版码表
│
├── assets/
│   └── tables/                 # 构建时自动复制的码表模板
│
├── deploy/                     # 打包输出目录
├── docs/                       # 设计文档
└── CHANGELOG.md                # 版本发布日志
```

## 技术栈

| 组件 | 技术 |
| :--- | :--- |
| 核心算法 | Rust（no-std 友好） |
| 配置格式 | TOML（serde 序列化） |
| Windows 输入法 | TSF（`windows-rs` 0.61） |
| 候选框渲染 | GDI（`UpdateLayeredWindow`） |
| 设置界面 | egui / eframe（wgpu/DX12） |
| TIP 管理 | COM：`ITfInputProcessorProfileMgr` |
| 无锁共享 | `arc-swap` + `parking_lot` |
| 日志 | `log` + `simplelog`（文件输出） |
| 打包 | Velopack（计划中） |

## 许可

MIT
