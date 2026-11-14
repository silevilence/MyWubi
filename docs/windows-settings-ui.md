# Windows 设置界面与自动化验证

设置程序负责日常输入配置，`skin_editor.exe` 负责 Windows / Android 皮肤制作。两个程序共用 egui 样式与核心文件校验；皮肤编辑器是同一 Cargo package 的独立 binary，随安装包和便携包一起分发。

## 设计依据与操作约定

本轮按 Double Diamond 梳理现有操作、确定优先级、实现并回归。依据是现有代码、实际渲染与自动化操作，未进行外部用户访谈。用户选定克制的 Windows 工具风格。

- **Discover**：常规页低频模型功能抢占首屏；皮肤编辑混在设置长表单中；部分控件丢输入或未写回；保存、刷新与即时生效操作的边界不清晰。
- **Define**：常用输入优先；设置按任务分组；编辑皮肤时参数与预览同时可见；用户必须能知道修改是否保存；基础回归无需人工点击。
- **Develop**：设置采用侧栏、分组卡片、固定保存栏；皮肤采用工作页签、独立预览、固定导出／应用栏。根据截图修正字体过细、滑轨不可见和管理操作挤占首屏的问题。
- **Deliver**：两套程序的命令接口、egui 事件测试、原生截图入口和打包脚本共同交付。

设置中的「保存更改」和 `Ctrl+S` 保存配置及码表草稿。自造词编辑窗口的新增、更新、删除、导入会立即保存，并明确显示这一规则。皮肤的「导出」创建新文件、不覆盖原包；「应用」读取最新配置，仅替换 Windows 皮肤选择，不改变其他设置。

## 无窗口操作接口

先构建：

```powershell
cargo build -p settings --offline --locked
.\scripts\test-settings-ui.ps1
```

脚本创建独立目录，复用仓库内的表情资源，生成小型测试码表，运行设置保存／重载以及皮肤创建／编辑／导出／应用／重开。结果写入 `target/settings-verification-*/`，包含请求、JSON 回执和输出皮肤；不安装 TIP、不修改正常用户配置、不下载模型或组件。

可直接向程序传入 JSON 请求文件：

```powershell
.\target\debug\settings.exe --automation C:\tests\request.json --output C:\tests\report.json
.\target\debug\skin_editor.exe --automation C:\tests\skin-request.json --output C:\tests\skin-report.json
```

```json
{
  "config": "C:\\tests\\config.toml",
  "actions": [
    {"action": "navigate", "page": "basic"},
    {"action": "set", "field": "basic.candidate_count", "value": 7},
    {"action": "save"},
    {"action": "reload", "discard": false}
  ]
}
```

`config` 必须是已有、有效配置的绝对路径。输出回执必须是新文件，避免覆盖原文件或旧证据。请求按顺序执行，第一个错误即停止；之前已经成功的文件操作不会回滚。回执包含每步配置、页面、修改状态和错误信息。退出码：`0` 成功，`1` 操作失败，`2` 参数、请求读取或回执写入失败。

| 程序 | action | 参数 |
| --- | --- | --- |
| 两者 | `inspect` | 无，返回当前状态 |
| settings | `navigate` | `page`: basic / appearance / dictionary / special_input / intelligence / tip_manager / about |
| settings | `set` | `field`: 已存在的标量字段路径；`value`: JSON 值，修改前先校验 |
| settings | `select_table` | `path`: 码表路径，使用与 GUI 相同的编辑会话 |
| settings | `save` | 使用界面的保存校验；不启动模型或学习后台进程 |
| settings | `reload` | `discard`: 默认 false；存在草稿时必须显式 true |
| skin_editor | `skin_new` | `preset`: jade / plain / contrast |
| skin_editor | `skin_open` | `path`: 已有皮肤文件 |
| skin_editor | `skin_set` | `platform`、`component`、`dark`（默认 false）、`field`、`value` |
| skin_editor | `skin_export` | `path`: 尚不存在的输出文件 |
| skin_editor | `skin_apply` | 应用当前 Windows 外观组件 |
| skin_editor | `skin_discard` | 明确丢弃草稿 |

`skin_new` 后组件 ID 与 preset 相同，例如 `jade`。修改已安装包的内容时，使用新版本或新包 ID，以满足皮肤仓库不覆盖同版本内容的规则。文件接口不提供 TIP 注册、提权、在线更新或任意命令执行。

## 原生界面截图

```powershell
.\scripts\test-settings-ui.ps1 -Capture

# 定位单页，便于有针对性的人工检查
.\target\debug\settings.exe --config C:\tests\config.toml --page appearance
.\target\debug\skin_editor.exe --config C:\tests\config.toml --open C:\tests\example.mywubi-skin
```

`--capture-ui <新目录>` 必须与 `--config <测试配置>` 一起使用。设置程序自动捕获七个页面以及 820×620 的常规／外观页；皮肤编辑器自动捕获空状态、浅色、深色、Android 外观、键盘组件以及 1000×660 的布局。采集完成自动退出，脚本检查截图数量，超时终止自己启动的测试进程。

截图尺寸为逻辑尺寸乘以系统缩放。截图是原生渲染结果，需要审阅可读性、裁切、滚动和分组；文件生成成功本身不代表视觉验收通过。

## UX 与 better-ui 审查记录

表中 HIGH 为操作受阻或可能丢失修改，MEDIUM 为可见一致性问题。以下问题已修正；位置指主要实现入口。

| Severity | Location | Before | After | Why |
| --- | --- | --- | --- | --- |
| HIGH | `windows/settings/src/panels/basic.rs:21` | 用 ComboBox 外层 response 判断选择，可能不写回 | 比较选择前后的值，写回并标记修改 | 操作应产生可观察的状态变化 |
| HIGH | `windows/settings/src/ui_style.rs:91` | 颜色或目录输入每帧从模型重建 | 保留输入缓冲；目录显式打开；非法颜色阻止保存 | 输入不能在打字中丢失 |
| HIGH | `windows/settings/src/app.rs:200`、`app.rs:353`、`app.rs:385` | 错误提示一帧后消失；刷新直接丢草稿；关闭确认可被绕过 | 错误持续显示；刷新和关闭有明确选择；Ctrl+S 与点击保存共用校验 | 反馈持久、用户可恢复和可取消 |
| HIGH | `windows/settings/src/save.rs:4`、`skin_editor.rs:239` | 独立进程可能覆盖旧配置 | 设置保存前核对磁盘快照；应用皮肤读取最新配置 | 保留其他窗口已保存的修改 |
| HIGH | `windows/settings/src/keyboard_editor.rs:38`、`skin_editor.rs:94` | 未写入组件的键盘编辑不计入脏状态 | 计入草稿状态，禁止切换目标或导出，提供放弃组件修改 | 防止切换与退出丢失编辑 |
| HIGH | `windows/settings/src/panels/dictionary.rs:8`、`dictionary.rs:504` | 初始化覆盖已有码表；即时删除没有确认 | 只补全缺少文件；删除词条二次确认；说明即时保存 | 破坏性操作与普通编辑分开 |
| HIGH | `windows/settings/src/app.rs:297` | 下载进度事件提前释放 worker；重启可能丢草稿 | 等终态再释放；有草稿时禁用更新／提权重启 | 状态反馈持续，重启前保留工作 |
| MEDIUM | `windows/settings/src/ui_style.rs:11`、`fonts.rs:9` | 默认表单密度、细字重、浅色错误文本、样式不一致 | 系统常规字体、统一间距／圆角／强调色、可读状态色 | 表面与控件状态保持一致 |
| MEDIUM | `windows/settings/src/app.rs:71`、`panels/basic.rs:188` | 高频输入设置被实验功能挤占 | 常用输入优先，预测与学习独立成页 | 缩短找到常用设置的路径 |
| MEDIUM | `windows/settings/src/skin_app.rs:31`、`skin_editor.rs:415` | 长表单中的预览和操作需要反复滚动寻找 | 独立 EXE；工作页签、常驻预览、固定导出／应用操作 | 编辑结果与动作同时可见 |

界面不添加位移、缩放或进入动画；页签即时切换，状态由文字与颜色共同表达。CSS、SVG 和浏览器动画检查不适用于 egui。

## 验证边界

已执行：settings 包单元／集成测试、实际 egui 下拉选择与跨帧文本输入测试、两个 EXE 的 JSON 流程、严格 Clippy、两套程序的原生截图检查。已有的真实模型服务交接测试保持 ignored，避免触发当前用户服务或重复准备模型。

Not verified：UAC 弹窗和真实 TIP 安装／卸载、在线更新下载及重启、各 GPU／模型后端、屏幕阅读器、系统高对比模式、原生文件对话框的人工交互。这些操作的审查不等于本轮执行了系统级验收。

Approve：仅限上述已检查范围；未验证项目不作通过结论。
