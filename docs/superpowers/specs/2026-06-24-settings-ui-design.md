# Windows 配置程序（settings.exe）UI 与持久化设计规格

- **日期**: 2026-06-24
- **范围**: `windows/settings` 模块——egui/eframe 配置界面、三个配置面板、config.toml 持久化
- **状态**: 待审核
- **关联 ROADMAP**: "配置界面基础 UI 框架与主题搭建"、"配置表单与交互开发"、"配置持久化与同步"

---

## 1. 目标与非目标

### 目标

- 在 `windows/settings` 中实现基于 `egui`/`eframe` 的图形化配置程序，替代当前 CLI 骨架
- 采用"固定窄侧边栏（160px）+ 右侧主配置区域"布局，包含四个面板：常规设置、外观样式、码表与词库、关于
- 实现三个配置面板的表单与交互：
  - 常规设置：候选词个数、上屏方式、中英文切换键、四码唯一自动上屏
  - 外观样式：字体大小、主色/背景色/高亮色（预设色块 + 系统取色器 + ARGB 文本输入三联动）
  - 码表与词库：码表/词库路径（`rfd` 浏览）、精确匹配/模糊音开关、用户词库功能开关（入口弹"待开发"）
- 实现 `config.toml` 的安全读写（复用 `core_engine::Config::save` 的临时文件 + rename 原子写）
- 配置变更通过"文件即真相"机制热重载——settings.exe 只写文件，`im_engine.dll` 现有 `notify` + `ArcSwap` 自动感知
- 内嵌开源中文字体子集，防止豆腐块乱码
- 显式"保存"按钮 + 未保存确认框，防丢失

### 非目标

- 不含 TIP 注册/启用/禁用/卸载功能（独立 spec）
- 不含用户词库的词条 CRUD/导入导出（需 `core_engine` 扩展，独立 spec）
- 不含 Android 配置界面（Kotlin/Compose 独立实现）
- 不含配置界面的截图对比自动化测试
- 不含鼠标点击候选词上屏等候选框交互（属 im_engine 范畴）

---

## 2. 关键决策汇总

| 决策点 | 选择 | 理由 |
|---|---|---|
| 范围 | UI 框架 + 三个配置面板 + 持久化 | 聚焦"配置功能"，TIP 注册属另一层复杂度 |
| 通知机制 | 文件即真相（依赖 im_engine 现有 notify+ArcSwap） | 架构最简，无跨进程耦合，百毫秒级延迟足够 |
| 配置路径 | exe 同目录优先，回退 `%APPDATA%\MyWubi\` | 便携模式 + 用户隔离双支持 |
| 文件对话框 | `rfd` crate | egui 社区事实标准，跨平台原生体验 |
| 词库功能 | 本期只做路径配置 + 开关；自造词留接口弹"待开发" | 遵循 AGENTS.md 修改边界，词库 CRUD 属 core_engine 独立任务 |
| 中文字体 | 内嵌开源中文字体子集（`include_bytes!`） | 确定性，任何 Windows 不乱码 |
| 保存模式 | 显式"保存"按钮 + 未保存确认框 | 与原子写基调契合，防误改无回旋 |
| 布局 | 固定窄侧边栏 160px | 与 Windows 系统设置/VS Code 心智模型一致 |
| 配色选择器 | 预设色块 + 系统取色器 + ARGB 文本输入联动 | 兼顾普通与高级用户 |
| 代码结构 | 方案 1：`egui::SideBar` + match 分发，适度模块拆分 | 贴合即时模式心智，避免过度抽象 |
| 日志 | `log` + 文件 appender 写到软件目录下 `log/settings.log` | 与 im_engine 隐藏日志风格一致 |

---

## 3. 模块结构与文件组织

```
windows/settings/
├── Cargo.toml              # 新增 eframe/egui, rfd, dirs, simplelog/fern 依赖
├── assets/
│   └── fonts/
│       └── noto_sans_sc_subset.ttf   # 内嵌中文字体子集（include_bytes!）
└── src/
    ├── main.rs             # 入口：初始化日志 → 解析配置路径 → 启动 eframe::App
    ├── app.rs              # SettingsApp：实现 eframe::App，编排侧边栏+面板+保存栏
    ├── state.rs            # AppState { config, dirty, active_panel, config_path }
    ├── config_path.rs      # resolve_config_path()：exe 同目录优先，回退 AppData
    ├── fonts.rs            # load_chinese_fonts()：注入内嵌字体到 egui FontDefinitions
    ├── save.rs             # 保存逻辑 + 未保存确认框 + 错误提示
    ├── log.rs              # 日志初始化：文件 appender 写 log/settings.log
    └── panels/
        ├── mod.rs          # pub enum Panel { Basic, Appearance, Dictionary, About }
        ├── basic.rs        # 常规设置面板
        ├── appearance.rs   # 外观样式面板
        ├── dictionary.rs   # 码表与词库面板
        └── about.rs        # 关于面板
```

**职责边界**：
- `app.rs` 只做编排（侧边栏 + 面板分发 + 保存栏），不写具体表单逻辑
- `state.rs` 是唯一状态容器，面板函数通过 `&mut AppState` 读写
- `config_path.rs` / `fonts.rs` / `save.rs` / `log.rs` 是纯函数工具模块，可独立测试
- 每个面板文件只负责自己的 UI 渲染，互不依赖

**新增依赖**（`windows/settings/Cargo.toml`）：
- `eframe`（含 egui）— GUI 框架
- `rfd` — 文件对话框
- `dirs` — 获取 `%APPDATA%` 路径
- `simplelog` — 文件日志 appender（按日期轮转）
- `windows-rs`（workspace 已有）— 调用 Win32 `ChooseColor` 原生颜色对话框

---

## 4. 配置路径定位与持久化

### 4.1 路径定位逻辑（`config_path.rs`）

```
resolve_config_path() -> Result<PathBuf>:
  1. exe_dir = current_exe().parent()
  2. 若 exe_dir/config.toml 存在 → 返回 exe_dir/config.toml  （便携模式）
  3. 否则 appdata = dirs::config_dir()/MyWubi/  （即 %APPDATA%\MyWubi\）
     - 若目录不存在则创建
     - 若 appdata/config.toml 不存在，从 exe_dir/config.toml 或内置默认复制一份
     - 返回 appdata/config.toml
  4. 若 AppData 目录创建失败 → 回退到 exe_dir/config.toml（便携模式），状态栏提示"已切换便携模式"
```

### 4.2 持久化流程（`save.rs`）

1. 用户点"保存"按钮 → 调用 `state.config.save(state.config_path)`
2. `Config::save`（core_engine 现有）实现"临时文件 + rename"原子写，无需改动
3. 写入成功 → `state.dirty = false`，底部状态栏显示"✅ 已保存"
4. 写入失败 → egui 弹窗显示错误，`dirty` 保持 `true`，不丢失未保存内容

### 4.3 未保存确认（`app.rs` 处理关闭）

- 关闭时若 `dirty == true` → 弹三选一对话框：**保存** / **不保存** / **取消**
  - 保存：走保存流程后退出
  - 不保存：直接退出
  - 取消：`ctx.send_viewport_cmd(ViewportCommand::CancelClose)` 阻止关闭
- 标题栏显示 `*` 前缀表示未保存（如 `*MyWubi 设置`）

### 4.4 dirty 追踪

面板函数修改 `state.config` 后置 `state.dirty = true`；保存成功后置回 `false`。

### 4.5 热重载约定

settings.exe 只写文件，不主动通知。`im_engine.dll` 现有 `notify` 监听器自动感知并 `ArcSwap` 原子更新。

---

## 5. 各面板 UI 细节

### 5.1 常规设置面板（`panels/basic.rs`）

| 配置项 | 控件 | 说明 |
|---|---|---|
| `candidate_count` | `Slider` 1..=10 | 候选词个数，拖动即时改值 |
| `commit_mode` | `ComboBox` | 选项：空格首选上屏 / 回车上屏，映射 `CommitMode` 枚举 |
| `switch_key` | `ComboBox` | 选项：Shift / CapsLock / Ctrl+Space，映射 `SwitchKey` 枚举 |
| `auto_commit_unique` | `Checkbox` | 四码唯一自动上屏开关 |

每个控件改动后置 `state.dirty = true`。ComboBox 用中文标签显示，内部映射到枚举值。

### 5.2 外观样式面板（`panels/appearance.rs`）

| 配置项 | 控件 |
|---|---|
| `font_size` | `Slider` 8..=32 |
| `primary_color` | 预设色块 + 取色器按钮 + ARGB 文本输入 |
| `background_color` | 同上 |
| `highlight_color` | 同上 |

**配色选择器交互**（每个颜色一行）：
1. 左侧色块预览（实时反映当前值）
2. 6-7 个预设色块一键选择
3. "自定义…"按钮 → 调用 Win32 `ChooseColor` 原生颜色对话框（经 `windows-rs` FFI），返回 RGB 后补全 Alpha 通道（默认 0xFF）
4. 右侧 `TextEdit` 单行，接受 `#RRGGBB` 或 `0xAARRGGBB` 格式，失焦/回车解析
5. 三者（色块/取色器/文本框）任一改动，同步更新 `state.config` 并刷新预览

**实时预览**：面板顶部放一个迷你候选框 mockup，用当前三个颜色 + 字体大小渲染，所见即所得。

### 5.3 码表与词库面板（`panels/dictionary.rs`）

| 配置项 | 控件 |
|---|---|
| `system_table` | `TextEdit` 路径 + "浏览…"按钮（`rfd::FileDialog::pick_file`） |
| `user_table` | 同上 |
| `enable_exact_match` | `Checkbox` |
| `enable_fuzzy` | `Checkbox` |
| `enable_user_dict`（新增） | `Checkbox` 开关 |
| 管理自造词 | "管理自造词…"按钮 → 弹"本功能待开发"提示 |

**路径浏览**：`rfd` 在后台线程调用避免阻塞 UI，结果回传主线程更新 `state.config`。

### 5.4 关于面板（`panels/about.rs`）

- 版本号（从 `Cargo.toml` 编译期 `env!`）
- 项目简介 + 仓库链接
- 当前配置文件实际路径展示（方便用户定位）
- 内嵌字体/依赖致谢

---

## 6. Config 结构变更

`core_engine/src/config.rs` 的 `DictionaryCfg` 新增字段：

```rust
pub struct DictionaryCfg {
    // ...existing fields...
    /// 启用用户词库功能（词库数据在 config 外单独保存，后续实现）。
    #[serde(default)]
    pub enable_user_dict: bool,
}
```

- 默认 `false`，`#[serde(default)]` 保证旧配置文件向后兼容
- 需同步更新 `Default for DictionaryCfg` 实现
- 补充 `roundtrip_toml` 测试覆盖新字段

---

## 7. 字体加载与日志

### 7.1 中文字体加载（`fonts.rs`）

- `assets/fonts/noto_sans_sc_subset.ttf`：Noto Sans SC 子集，仅含常用 GB2312 字符 + ASCII，体积控制在 ~1-2MB
- `load_chinese_fonts(ctx: &egui::Context)`：
  1. `include_bytes!("../assets/fonts/noto_sans_sc_subset.ttf")` 编译期内嵌
  2. 注入到 `egui::FontDefinitions::default()` 的 `FontFamily::Proportional` 和 `FontFamily::Monospace`
  3. 设置为最高优先级，确保中文优先用内嵌字体而非系统回退
- 在 `SettingsApp::new()` 中调用一次

### 7.2 日志（`log.rs`）

- 使用 `log` + `simplelog` 文件 appender
- 输出到软件目录下 `log/settings.log`，按日期轮转
- 与 `im_engine.dll` 的隐藏日志风格保持一致
- 默认 `Info` 级别，`RUST_LOG=debug` 环境变量可提升级别

---

## 8. 错误处理策略

| 场景 | 处理 |
|---|---|
| 配置文件不存在 | `resolve_config_path` 自动创建默认配置（`Config::default()`），不报错 |
| 配置文件解析失败 | 启动时弹 egui 窗口显示错误详情 + "加载默认配置" / "打开文件位置" 按钮；选默认则用 `Config::default()` 覆盖损坏文件 |
| 保存失败（权限/磁盘满） | egui 模态弹窗显示错误，`dirty` 保持 `true`，不退出 |
| `rfd` 文件对话框取消 | 静默忽略，路径不变 |
| ARGB 文本输入非法 | 输入框标红 + tooltip 提示格式，不更新 `state.config` |
| AppData 目录创建失败 | 回退到 exe 同目录（便携模式），状态栏提示"已切换便携模式" |

---

## 9. 测试策略

### 9.1 单元测试（纯函数模块）

| 模块 | 测试点 |
|---|---|
| `config_path.rs` | exe 同目录存在时返回便携路径；不存在时回退 AppData 并创建；AppData 创建失败回退 exe 目录 |
| `save.rs` | 保存成功后 `dirty` 置 false；保存失败 `dirty` 保持 true（用临时目录模拟） |
| ARGB 解析 | `#RRGGBB` / `0xAARRGGBB` 合法格式解析正确；非法格式返回错误 |

### 9.2 集成测试

- `core_engine` 侧：新增 `enable_user_dict` 字段后，补充 `roundtrip_toml` 测试覆盖新字段，确保序列化/反序列化 + 默认值正确
- 配置路径定位：模拟"无 exe 同目录配置"场景，验证 AppData 回退 + 默认配置生成

### 9.3 手动验收（不自动化）

- 启动 settings.exe，切换四个面板无崩溃
- 改候选词个数 → 保存 → 检查 config.toml 实际更新
- 配色三联动（色块/取色器/文本框）一致性
- `rfd` 浏览选码表路径 → 保存生效
- 关闭时未保存确认框三选项行为正确
- 中文字体无豆腐块乱码
- 改 config.toml 后 im_engine 热重载生效（跨进程验证）

### 9.4 不做的事

- 不对 egui 渲染做截图对比测试（egui 生态无成熟方案，ROI 低）
- 不做 TIP 注册相关测试（本期范围外）

---

## 10. 后续 ROADMAP 待办项（需用户确认后落盘）

本 spec 衍生以下后续任务，建议加入 ROADMAP：

### 10.1 im_engine 配置路径同步

- [ ] `im_engine.dll` 的 `notify` 监听路径需采用与 settings.exe `resolve_config_path` 相同的定位逻辑（exe 同目录优先，回退 `%APPDATA%\MyWubi\`）
- [ ] 确保双进程读写同一份 `config.toml`，避免便携模式与用户模式路径不一致导致热重载失效

### 10.2 用户词库功能（core_engine + UI 联调）

- [ ] `core_engine` 设计用户词库数据结构（独立于 config.toml 的存储格式）
- [ ] `core_engine` 暴露用户词库 CRUD / 导入导出 C-ABI 接口
- [ ] settings.exe 码表面板"管理自造词…"按钮对接真实功能（替换"待开发"提示）
- [ ] im_engine 侧用户词库的检索集成与热重载

### 10.3 TIP 注册/启用集成（独立 spec）

- [ ] 在 settings.exe 中通过 `ITfInputProcessorProfileMgr` COM 接口实现 TIP 注册/启用/禁用/卸载
- [ ] 设置工具启动时检测 TIP 注册状态并提示用户