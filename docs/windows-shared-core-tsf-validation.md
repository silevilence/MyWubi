# Windows 共享核心迁移：Sandbox TSF 验收（CP-10）

日期：2026-10-30（Asia/Shanghai）。任务来源：[ROADMAP CP-10](../ROADMAP.md)。

本次使用 Windows Sandbox 中的真实记事本和 Microsoft Edge，注册新构建的
`im_engine.dll`，通过逐键事件验证 TSF。没有用粘贴中文、模拟 Editor、DOM 赋值或
历史验收记录替代输入。CP-10 已完成：浏览器语言栏最终复核在桌面解锁后通过，
本次覆盖的场景未发现新增回归，异常表现已用迁移前 DLL 对照。以下明确保留既有宿主限制，不把兼容验收等同于缺陷全部修复。

## 环境与制品

| 项目 | 实测值 |
| --- | --- |
| 当前源码 | `fb6a261e97b4d8efc80c7d253c83b7a7bf4d82f4`；验收过程中未修改生产代码 |
| 迁移前对照源码 | CP-01 `201266706ef67d45d1755a0fbbb01b0c5c7702a2` |
| 工具链 | rustc 1.99.1 / cargo 1.99.1；`--release --locked --offline` |
| Sandbox CLI | `wsb 0.8.107.0` |
| Sandbox 系统 | Microsoft Windows 11 Enterprise，10.0.26100，x64 |
| 记事本 | Microsoft 经典记事本，文件版本 10.0.26100.8457 |
| 浏览器 | Microsoft Edge 156.0.4314.50，原生 HTML textarea |
| 设置与打包 | settings 0.4.0；Velopack CLI 1.2.0 |
| 新版安装目录 | Sandbox 内 `C:\MyWubiQA\` |
| 对照安装目录 | Sandbox 内 `C:\MyWubiBaseline\` |
| 便携配置 | 主表 `tables/main.dict`，初始候选数 2，PageDown / PageUp 翻页 |
| 码表 | `core_engine/tests/fixtures/temporary/` 的 main / english / pinyin 三个小型表 |

Sandbox 本身没有可启动的记事本，因此复制了主机 WinSxS 中 Microsoft 的 x64
经典记事本及 en-US / zh-CN MUI 到 Sandbox 的 `C:\MyWubiQA\notepad\`。
它是真实系统文本宿主，不是自制测试编辑器。其默认 Lucida Console 显示中文方框，
改选 Sandbox 已有的 Microsoft YaHei UI 后正文正常；没有改动输入法字体实现。
本次不覆盖商店版 Notepad。

### SHA-256

| 制品 | SHA-256 |
| --- | --- |
| 实测、打包及安装后的 `im_engine.dll` | `B93653541055C730D881F92CD315DF49C7BBF01EFC5B900BB39368285A27F3C6` |
| 实测、打包及安装后的 `settings.exe` | `756308A284175DEDC712E0653E4F8E7833F9923476244601F147AD156DE342F5` |
| 迁移前对照 DLL | `C22668D08C6DE20DEF47311C6B96C28AF170A1B855072826AF0809D1C2A30A66` |
| 初始配置 | `B49B609070A673277C6DB2FC778CB65B4A1A80ECA59BEDE09EC61DE4098B941A` |
| 最终配置 | `F2677236F340A47382290AE7B3DC058E2127234C19BB08BE29FE2C98DEE6F719` |
| Setup.exe | `1549071B68707E7EE57AFFE83BCF3D48399B65D3AFDE089C842015D2DF79F3B8` |
| Portable.zip | `56150F6300D6686399A0E696C80E199270FCAC47124C4460BE2EAD48CDB27C87` |

实测制品在构建后立即冻结到主机 `target/cp10-sandbox/`。随后构建旧版作对照，
并重新构建当前源码；打包使用冻结的实测制品，不能把后一次构建的文件哈希混入本记录。
安装后再次读取文件哈希，确认包内 DLL / EXE 与实测文件相同。

配置保存会规范化 TOML，因此初始和最终配置的字节哈希不同。最终已恢复候选数 2、
反查开启及原英文辅助表。[初始配置](validation/cp10/config.initial.toml)、
[最终配置](validation/cp10/config.final.toml)、[最终主表](validation/cp10/main.final.dict)
和 [Sandbox 内文件哈希](validation/cp10/guest-final-hashes.json) 一并保留。

## 真实宿主结果

下表中的反引号表示配置的临时入口键；各字母均通过逐键输入，未粘贴编码。

| 场景 | 实测操作与结果 |
| --- | --- |
| 普通形码 | 两个宿主均能 `a` → GDI 显示“主” → 空格提交“主”；候选不抢走输入焦点 |
| 临时菜单 | Edge 反引号显示“1 拼音 / 2 英文”，菜单标签未写入正文 |
| 拼音与分页 | Edge 反引号、1、`ni` 显示你 / 你好；PageDown 显示你好吗，PageUp 返回 |
| 部分音节分隔 | 两个宿主均通过 `ni'h`、空格提交“你好”；`wg / wq / wqvb` 反查提示没有混入正文 |
| 返回主模式 | 拼音提交后 `a`、空格仍提交主表“主” |
| 临时英文 | 两个宿主反引号、2、`brb`、2 提交 `be right back soon`，无额外尾空格 |
| 取消与退格 | Edge Esc 删除本轮预编辑；记事本 `n` → 退格 → 退格退出空临时模式，之后 `a`、空格提交“主” |
| 英文回车 | 记事本临时英文 `b`、回车提交原文 `b`，没有选择词组或输入模式标签 |
| 撤销 | 记事本 Ctrl+Z 可撤销连续输入；Edge `a`、空格、Ctrl+Z 恢复选中的 `a`，再次 Ctrl+Z 清空；旧 DLL 同样如此 |
| Shift 与语言栏 | 记事本、Edge 均验证 Shift 切英文后逐键 `hi` 直接输入，点击语言栏切回中文后 `a` 显示“主”候选，空格提交“主”；候选不残留 |
| 切焦点 | Edge A→B、记事本→其他窗口均隐藏候选且不向新宿主串写；临时 `b` 留在旧宿主的问题在新旧 DLL 均复现，见下节 |
| 码表保存热重载 | 设置程序关闭反查、保存主表后，同一 Edge 进程提示消失；恢复反查并保存后重新出现 |
| 配置保存热重载 | 设置程序候选数 2→3 并保存，同一 Edge 和记事本进程的 `ni` 均显示三个候选；结束时保存回 2 |
| 故障与恢复 | 原子替换英文表为非法 YAML 后，日志报告保留旧快照，Edge `brb` 仍显示两个英文词组；恢复文件后可重新发布 |
| 保存失败草稿 | 辅助表非法时保存主表未覆盖磁盘旧值，窗口保留未保存标记；辅助表恢复后再次保存成功，反查恢复 |
| TIP 管理 | `regsvr32 /s` 注册成功；设置程序显示已安装、已启用、DLL 指向 QA 目录；禁用及重新启用状态均正确 |

主要截图：[当前 Edge](validation/cp10/edge-current.png)、
[关闭反查](validation/cp10/edge-reload-off.png)、
[失败重载仍有候选](validation/cp10/edge-failed-reload.png)、
[Edge 恢复反查及三个候选](validation/cp10/edge-reload-restored.png)、
[记事本热重载](validation/cp10/notepad-reload-restored.png)、
[设置安装状态](validation/cp10/settings-installed.png)、
[Edge 英文旁路](validation/cp10/edge-languagebar-english.png)、
[Edge 语言栏切回中文](validation/cp10/edge-languagebar-chinese.png)。

最后一次复核使用原 Edge 进程 PID 20284，重新核对其模块仍为
`C:\MyWubiQA\im_engine.dll`，哈希仍为上表的 `B9365354…A27F3C6`。
测试页 Text B 原有 `hi`，复核先中文提交“主”、Shift 后输入 `hi`，
再点击语言栏切回中文提交“主”，最终为 `hi主hi主`；没有粘贴这段结果文字。

## 旧版对照与已知限制

对照 DLL 来自迁移前 CP-01 的真实源码，使用同一工具链构建。旧、新 DLL 分别放在不同目录，
Edge 使用不同 `--user-data-dir` 启动两个独立进程，记事本也启动两个独立进程。
[模块路径记录](validation/cp10/loaded-modules.txt) 确认当前 Edge PID 20284、
记事本 PID 16704 加载 `C:\MyWubiQA\im_engine.dll`；对照 Edge PID 5816、
记事本 PID 12096 加载 `C:\MyWubiBaseline\im_engine.dll`。

1. **临时预编辑失焦残留。** Edge 从 A 文本框切到 B，或经典记事本切到另一窗口时，
   临时英文 `b` 会留在旧正文，候选隐藏。旧 DLL 也复现，不能把它报告为本次迁移已修复，
   更不能套用历史 Windows 10 的“失焦删除”结论。
   [旧 Edge](validation/cp10/edge-baseline.png) 与
   [旧记事本](validation/cp10/notepad-baseline.png) 保留证据。
   本轮没有加入 COM 回调级跟踪，因此不把具体宿主终止顺序写成已证实的根因。
2. **Edge 撤销分两步。** 第一次撤销提交恢复编码，第二次清空；两版相同，菜单及提示未进入撤销正文。
3. **非原子外部写入的空表窗口。** 最初用 PowerShell `Set-Content` 截断后写入非法辅助表时，
   监听器先读到合法空文件并发布，随后才拒绝非法 YAML，英文候选因此暂时消失。
   旧版也接受空辅助表并显示无候选，[对照截图](validation/cp10/edge-baseline-empty-table.png)。
   恢复完整文件后候选恢复。正式失败快照检查改用同目录临时文件加 `File.Replace`，
   避免把写入中间态当作完整坏文件；应用自身原子保存与外部非原子编辑不能混为一谈。

以上是本环境中已解释的既有行为，不是共享核心迁移产生的新旧差异。
W15 的纯会话测试与真实宿主外部终止路径存在覆盖边界；本记录补充该边界，不改写冻结测试的断言。
本次未覆盖商店版 Notepad、AppContainer 文本宿主、多显示器、DPI 组合、全部键盘布局、
全部 W01～W24 的逐项实机故障注入，以及 Android。它也不替代 CP-24 对最终制品的重新验收。

## 构建、安装与整体检查

| 检查 | 结果 |
| --- | --- |
| `cargo test --workspace --quiet --locked --offline` | 274 通过，0 失败，0 忽略 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo build --workspace --release --locked --offline` | 通过；已有 LNK4104 导出警告仍存在 |
| 原版 `build.ps1 -SkipBuild` | 在独立暂存副本中成功生成 0.4.0 Setup / Portable / full.nupkg |
| Sandbox 中 `MyWubi-win-Setup.exe --silent` | 返回 0；安装到 `%LOCALAPPDATA%\MyWubi\current`，DLL / EXE 哈希吻合 |
| TIP 注册与管理面板 | 注册成功；安装路径、启用/禁用状态核对通过 |
| 最终证据审核 | 报告本地链接、配置/主表哈希、加载路径、完成标记与截图一致；`git diff --check` 通过 |

本轮交付只增加验收记录、测试页面和证据文件，并更新 CP-10 状态；生产代码、依赖及版本未变。
因此沿用本次同源码、同实测制品的 274 项测试及构建结果，没有为文档变更重复执行无关测试。

打包使用原版 `build.ps1`，仅把执行根目录隔离到
`target/cp10-package/`，该目录初始不存在 staging，不触碰仓库原有 `Releases/`。
没有发布、推送、升级 vpk 或修改版本号。由于隔离输出目录没有旧包，只验证完整包，
未声称生成或测试增量升级包。未验证安装器卸载与自动更新下载。

完整主机日志位于忽略目录 `target/cp10-tests.log`、`target/cp10-build.log`、
`target/cp10-final-build.log`、`target/cp10-package.log`；
[运行时原始日志](validation/cp10/engine.log) 随文保存。
多个宿主向同一文件写日志，部分行交错；日志不含 PID，不能把每条 revision 凭空归给某个进程。
热重载结论同时由原宿主窗口中的可见候选验证。

## 复现入口

1. 构建当前源码，冻结 DLL / EXE 并计算哈希。将这两个文件、
   [初始配置](validation/cp10/config.initial.toml)、
   [浏览器页面](validation/cp10/browser.html) 与三个 fixture 码表复制到专用交换目录。
2. 使用 `wsb start --raw` 创建新 Sandbox；`wsb connect --id <id>` 打开桌面，
   `wsb share --id <id> --host-path <专用交换目录> --sandbox-path C:\QAExchange --allow-write`。
   只共享测试目录，不共享整个用户目录。首次启动可能需要数分钟，交互用户就绪前
   `--run-as ExistingLogin` 会返回 `0x80070520`，需等待桌面出现。
3. 通过 `wsb exec --id <id> --run-as ExistingLogin --command <命令>` 将制品复制到
   Sandbox 的 `C:\MyWubiQA`，在其中执行
   `C:\Windows\System32\regsvr32.exe /s C:\MyWubiQA\im_engine.dll`。
   **不要在主机运行该注册命令。** 本次主机未注册测试 TIP。
4. 在 Sandbox 为中文语言加入
   `0804:{C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD8}{C9F2EAA4-0AB7-49C6-9F2C-8B8FA8D5FFD9}`，
   选择 MyWubi，分别打开记事本与 Edge 本地页面，执行上表。
   本次 Sandbox 的 `.ps1` 执行默认被禁用，改用受支持的独立 CLI 命令，未修改执行策略。
5. 对照时注册旧目录 DLL，仅为新进程创建实例；必须检查每个宿主实际加载的模块路径。
   对照后把注册恢复到当前版。保存配置、表文件、屏幕与安装后哈希，不能仅凭菜单名字判断版本。

浏览器页仅含两个原生 textarea 和事件显示，没有网络请求、自动提交或核心算法实现。
它用于观察真实 Edge TSF 行为，不代表其他网站、富文本编辑器或沙箱化文本宿主均已通过。
