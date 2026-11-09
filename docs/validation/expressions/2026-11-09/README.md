# EM 可复核证据（2026-11-09）

任务状态、测试环境和限制见 [验收记录](../../expressions.md)。这里保存全量字形报告、代表性截图、真实宿主输出及日志；不是发布包，也不是全部条目逐条视觉验收。

| 目录 / 文件 | 含义 |
| --- | --- |
| `windows/emoji-coverage.json`、`kaomoji-coverage.json` | 沙盒内实际 DirectWrite 全量字形 / 序列检查与失败列表 |
| `windows/sandbox-system.json`、`sandbox-artifacts.json` | 沙盒 OS、实际使用的程序及字体 / 渲染 DLL 摘要 |
| `windows/host-font-versions.json` | 主机字体版本与回退字体摘要，区别于沙盒元数据 |
| `windows/browser-*.png` | 真实 Edge 中两个必过查询的候选界面 |
| `windows/em-host-*.json` | Edge 宿主 input/composition/undo 事件及最终码点 |
| `windows/numeric-reload-*`、`resource-reload-*.png`、`gate-*.png` | 数字选词、空格/数字提交、实际词库切换与旧点击、关闭再开启输入接管的宿主证据及离线日志核对 |
| `windows/notepad-*.txt` | 真实记事本保存文件，UTF-8 回读与期望逐字符相等 |
| `windows/candidate-*.png` | 颜色、组合回退、长串、暗色字号和滚动 |
| `windows/query-*.png` | 部分匹配与无结果，二者不能混用 |
| `windows/settings-*.png` | egui 中使用系统字体渲染的实际预览 |
| `windows/sandbox-display-options-unavailable.png` | 沙盒不能完成实际 DPI 设置验收的环境记录 |
| `android-api26/`、`android-api34/` | 各系统全量覆盖、字体配置/摘要、Compose 代表性截图及真实外部宿主证据 |
| `android-api*/layout-test.log` | 横竖屏 × 1/2 倍 Compose 字体四项成功记录 |
| `android-api*/host-test.log`、`host-system-font2-test.log` | 默认和系统 2 倍字体下，外部 EditText 真实输入各一项成功记录 |
| `android-api*/*journal.jsonl` | 外部应用 TextWatcher 正文变化，包含测试主动清空及最终提交 |
| `android-api*/full-library-tail-*` | 完整词库末组选择、部分匹配及返回原模式的真实宿主证据 |
| `android-api*/numeric-reload-test.log`、`resource-reload-*` | 数字选词/选表情、实际词库切换、旧无障碍节点点击及新结果提交；不是 TalkBack 验收 |
| `android-api*/host-system-font2.png` | 修改实际系统 font_scale 后的 IME 查询界面 |
| `checks/` | 核心、Windows 输入法、设置、渲染及补充回归测试日志 |
| `package-verification.json` | 三个本地产物的摘要及 13 个分发文件逐字节核对 |

Android 带 `portrait/landscape` 前缀的截图来自真实 Compose Activity 测试；带 `host-` 前缀的截图来自独立外部 EditText 与真实 InputMethodService。两者分开保存，不把组件测试等同外部应用或真机验收。

Windows 事件日志显示最终只写入一份表情，但 Edge 有两次同值 composition 更新及两步撤销；该差异仍待调查。浏览器候选定位已在后续修复并复验；实际 DPI、真机及 TalkBack 等未通过项目仍留在 ROADMAP，不能凭这些截图勾选全部任务。

后续补验：`browser-caret-fixed.png`、`browser-automatic-local.png`、`browser-focus-cancelled.png`、`browser-stale-click-rejected.png` 与 `browser-boundaries-journal.json` 记录只读定位修复和边界检查。`host-boundaries-test.log` 包含 Android 视图隐藏修复后的失焦、重载、满码回归。`android-ordinary-initial.log` 保留旧测试假设导致的初次失败，修正后结果在 `android-benchmark-mode-regression.log`，两者不可混为一次全绿运行。

`windows/ordinary-undo-journal.json` 与 `mode-return-undo-journal.json` 是普通标点和表情两步撤销的对照；`selection-replacement-journal.json` 验证原选区仅在最终选择时被替换、撤销后恢复。`android-api26/numeric-reload-initial-failure.log/png` 单独记录补测首次未弹出键盘的准备失败，最终双系统成功日志不覆盖该记录。

`windows/NUMERIC-RELOAD-CHECKLIST.md` 是第四轮集中操作前准备的清单，实际重开菜单后进一步进入 Emoji 模式，确认关键词为空。`numeric-words-local.png` 与 `space-final-committed.png` 对应数字组合查询和空格提交；`resource-old-result.png` 及 `resource-reload-*.png` 对应重载前、重载后旧点击和新结果。单条 `emoji-reload-fixture.mwe` 是本地重载验收样本，实际选择保存在 `numeric-reload-active-config.toml`，不属于发布全集。最终日志只有两次表情提交与中间撤销，沙盒关闭后完成离线断言并保存 `numeric-reload-journal-check.json`。
