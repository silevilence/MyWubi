# 2026-11-13 SI 自动化与性能记录

本轮只执行命令行、无窗口 egui 帧和桌面 JVM JNI 检查。未安装/注册本轮 TIP，未切换窗口、点击或输入，未启动模拟器；所有构建资源来自本地缓存，零下载。Windows/Android 实际宿主、DPI、大字体和 TalkBack 待验，不能由以下结果替代。

## 固定用例与 JNI

`cases.json` / `workloads.json` 的 SHA-256 与 SI-01 冻结清单一致，见 [环境记录](environment.json)。核心测试直接读取固定期望，不再在 C01/C02/C06 中用另一遍插入/分页计算生成期望。快捷输入 15 项、规则 10 项、专用查询 16 项通过；字典定向 50 项（含筛选匹配到的用户词库测试）、运行时 5 项、拼音检索 5 项、普通输入 36 项通过。核心库及性能工具严格 Clippy 通过。

`scripts/validation/special_input_jni.py` 编译一个无窗口 Java 类，加载实际 `mywubi_android.dll`，经原 JNI `byte[]` 接口测试；结果与 DLL 摘要见 [jni-desktop.json](jni-desktop.json)。10 组、1168 次请求通过，不模拟 JNI 的编码/序列化或平台取时实现。Java 默认区域设置为 `th-TH`，运行时仍使用公历；时区变化只影响下一轮完整命中。查询期间没有真实宿主，编辑计划由脚本确认，因此这里不构成真实上屏证据。

| 固定样例 | 核心与接口证据 | 仍待设备/宿主证据 |
| --- | --- | --- |
| C01/C02 | 两个行为配置组的原页面/连续页面；JNI 比较固定页面 | 实际候选窗口与翻页 |
| C03 | 不足回退的固定序列，JNI 一致 | 实际页面落位 |
| C04/C05 | 普通/特殊/动态同文、独立 ID、学习标志；JNI 一致 | 真实点击/上屏与采集闭环 |
| C06 | 容量 1/9/10、全局 9/10/11、数字/ID 点击文本一致 | Windows 鼠标和 Android 触摸 |
| C07 | 规范编码、停用重复、限值、冲突暂停及恢复；JNI 保存失败不改磁盘 | 设置字段修正/换表操作 |
| C08/C09 | 可注入时钟验证精确跨日、调用次数、失败/重试/旧 ID；JNI 补充真实 GregorianCalendar、时区冻结、重载与不确定提交不重放 | Android 可注入时钟设备场景及真实宿主生命周期 |
| C10 | 两种查询方式的核心测试；JNI `shi`/`SHI`、编辑数字、禁止非空切换、结果数字选择 | 从真实类别入口完成两种方式 |
| C11 | 前缀、重叠、32 字母、失配/停用、自动上屏矩阵；JNI 长码与重叠编码 | 真实键盘输入轨迹 |

数字 `0` 在动态命中时不是第十候选快捷键。JNI 测试同时确认原缓冲标点策略仍可提交“首个普通候选 + 0”；不把“不能选择第十项”误写成按 0 必须无操作。

## Windows Release 核心性能

Intel Core Ultra 7 165U，Rust 1.99.1，系统版本见环境记录。与 SI-01 相同：每个独立进程预热 20 轮，输入测 1000 轮，候选合并测 200 轮，nearest-rank p95/p99，保留 OS 文件缓存。工具为 `core_engine/examples/special_input_benchmark.rs`，采样脚本为 `scripts/validation/measure-special-input.ps1`。没有同时运行本任务的构建或测试来竞争 CPU；用户其他工作保持运行，CPU 负载样本保留在 JSON，无法获取的值为 null。

最终原始数据位于 [binary-index](binary-index/core-memory.json)，下表取三轮各指标的最大值。不同测试场景完成后释放其快照，不在测量正式全集时同时持有已结束的合成压力运行时；测量进程的累计峰值仍不重置。

| 指标 | 三轮最坏值 | SI-01 门槛 | 结论 |
| --- | ---: | ---: | --- |
| 默认全关闭五字符轮次 p95 / p99 | 3.3 / 4.1 μs | 10 / 20 μs | 通过 |
| 1024 条规则全部停用，补充检查 p95 / p99 | 8.3 / 16.0 μs | 同上，保守对照 | 通过 |
| 1024 条未命中，固定 `jjad` p95 / p99 | 14.6 / 29.2 μs | 100 / 200 μs | 通过 |
| 1024 条未命中，五字符 `jjaad` p95 / p99 | 23.5 / 44.2 μs | 100 / 200 μs | 通过 |
| 六类别各 1024 条命中，合并 6145 项 p95 / p99 | 3.1944 / 4.0568 ms | 10 / 20 ms | 通过 |
| 同码 32 条动态规则 p95 / p99 | 0.0302 / 0.0651 ms | 10 / 20 ms | 通过 |
| 全内置资源 + 1024 条规则运行时构建 | 439.93 ms | 2000 ms | 通过 |
| 独立核心进程峰值工作集 | 120,262,656 bytes（114.7 MiB） | 128 MiB | 通过 |

固定压力文件的未命中输入是四字符 `jjad`，设计表另要求五字符轮次，因此两者均报告，不修改冻结文件。正式资源共 Emoji 3972、颜文字 34354、假名 224、数学 124、标点 49、其他符号 109 项；没有用合成小库替代正式全集加载。完整构建包含当前内置五笔、拼音、英文资源，资源/工具摘要见 `core-memory.json`。

### 保留的失败与修正过程

- 本目录的初始 `core-run-1～3.json` / `core-memory.json`：峰值约 253 MiB，超出 128 MiB；1024 条均停用的补充轮次有一轮 p95 超过 10 μs。默认关闭、未命中及两组候选合并门槛通过。
- `optimized/`：显式 Trie 查询改为首次调用才建树，空用户词库直接复用主表；峰值约 138 MiB，仍超限，补充停用轮次仍有延迟超限。
- `isolated-workloads/`：修正基准无意保留已结束场景快照的问题；峰值约 133 MiB，仍超限，不能宣称只修工具就达标。
- `binary-index/`：精确编码查询复用排序数组二分下界，去掉每个主/辅助表重复存储全部编码的 BTreeMap；三轮全部门槛通过。保留已有精确匹配权重顺序、前缀、万能键与拼音语义；显式 Trie 接口仍可用，多线程首次初始化只构建一次。相应字典/运行时/普通输入/拼音/快捷输入回归通过。

本次是实现修改后的重新测量，不是挑选慢样本或改变门槛。旧基线与中间失败结果均保留；桌面 JVM 的功能检查不计入 Android JNI 性能。Android API 34/x86_64 和 ARM64 的当前 JNI 延迟、PSS、构建耗时以及两端候选实际呈现时延尚未测量，SI-15 整体保持未完成。

## 实机验收限制

只调用一次 cua `getState()` 核查能力；返回文档明确“Windows input activates the selected window”。当前接口不能不抢焦点地执行真实 TSF 按键，故按用户“不占用电脑”的要求跳过输入操作。没有打开测试应用、截图、输入、改变焦点或注册 TIP。Android 未连接设备，未启动模拟器；设置/宿主设备测试仅编译准备。SI-16/17 保持待验。

## 最终构建与制品核对

统一执行 `cargo test --offline --workspace --lib --tests`：60 个测试程序、498 项通过、6 项既有真实模型/下载条件测试忽略，零失败，见 [workspace-tests.json](workspace-tests.json)。随后补充 Windows `input_session::tests::date_global_positions_publish_matching_keyboard_and_gdi_click_identities`，定向运行通过：18 种容量 1/9/10、位置 9/10/11、数字或点击路径组合，核对冻结 C06 页/槽、GDI 数据标签和日期提示、提交及旧点击拒绝。它使用真实 Windows 时间提供者和模拟 Editor，不是真实 TSF 或窗口点击；没有为此宣称整套测试重新执行。可在后台完成的检查已结束，后续真实环境按[集中清单](../host-checklist.md)执行。

本节制品的生产代码基点为 `72d410d`，到首次收尾 `5855b27` 仅补充测试和记录；下文审核修复不包含在这些旧制品中。Windows 使用缓存的 `target/v080-published/components.json` 设置 `MYWUBI_COMPONENT_CATALOG`，运行 `cargo build --offline --release --locked --jobs 2 -p settings -p im_engine` 成功。三份普通码表及六类特殊资源按共享清单复制；`settings.exe --component-catalog` 仅执行启动前的命令行分支并以隐藏窗口运行，输出与缓存的发布目录一致。

本地 Velopack 1.2.0 生成 `target/si-validation-20261113/packages/` 中的 Setup、Portable 和 nupkg，验收包版本 `0.8.0-si.20261113`，项目版本未更改。使用 `--skipVeloAppCheck true` 跳过需要启动应用的集成探测；该项和安装、升级行为仍待实机验证。没有执行安装器，没有签名或发布。可选模型组件不在此次特殊输入包内，也未重新下载组件归档；只复用已有组件目录元数据，不能据此宣称模型组件交付已验证。

Android 使用本地 JDK 21.0.8、SDK/NDK 和 Gradle/Cargo 缓存，`CARGO_NET_OFFLINE=true`、`CARGO_BUILD_JOBS=2`，执行 `gradlew.bat --offline --max-workers=2 :app:assembleDebug :app:assembleDebugAndroidTest --console=plain` 成功。主 APK 含 ARM64/x86_64 的 Release Rust 库，外壳为 Debug，设备测试 APK 仅编译，未安装执行。既有 SDK package.xml、Gradle 弃用和 Windows 导出链接提示未阻断构建。

`scripts/validation/check-special-input-artifacts.py` 对 Portable、nupkg、APK 内各 22 项资源/许可与源文件逐字节比较；Windows exe/DLL 对照本次 Release 产物。Android 打包会执行符号剥离，直接与 Cargo 输出比较的首次检查失败；确认两种 ABI 的合并输入均等于 Cargo 输出，并用本地 NDK `llvm-strip --strip-unneeded` 独立重放后，与 APK 内文件逐字节一致。此转换链已写入校验工具，不忽略差异。最终检查通过，完整路径、大小及 SHA-256 见 [artifacts.json](artifacts.json)。Setup 只记录容器摘要，不将生成成功写成已安装通过。

## 收尾后的审核与修复

审核基点 `5855b27`，重点变更范围 `7cbb88b..5855b27`，并对照 SI-01～SI-17 的共同规则、设计、冻结样例和当前平台接入检查。用户明确授权发现问题直接修复并提交，禁止占用桌面验收。本次审核没有调用 cua、启动设置窗口、安装 TIP、运行设备 UI 测试或下载资源；egui 检查只生成内存帧。

### 发现与逐条处理

| 编号 | 严重度 | 核实与影响 | 决策与修复 | 验证 |
| --- | --- | --- | --- | --- |
| Q1 | 🟠 Important | Windows 日期预览和检索试用已经完成 A 后，快速改为 B 再撤回 A，结果被清掉但完成标记仍为 A，永远不再启动计算 | 采纳；草稿键变化时同时清掉结果与完成标记，仍保留过期异步结果检查。见 [shortcuts.rs](../../../../windows/settings/src/panels/shortcuts.rs)、[expression_trial.rs](../../../../windows/settings/src/panels/expression_trial.rs) | 两个无窗口回归测试在修复前失败，修复后通过 |
| Q2 | 🟠 Important | 仅切换预测设置的热重载优化保留了已经完整匹配的日期快照，违反运行时重载结束旧查询的 SI-C09 规则；失败取时和 Touch 未发布重试也受影响 | 采纳日期相关修复；存在取时尝试或可重试快照时走正常重载清理，下一次完整匹配重新取时。见 [session.rs](../../../../core_engine/src/session.rs)。不移除普通输入既有热切换优化 | 原始复现先失败后通过；两配置组及成功、失败、未发布重试分支通过；原预测热切换测试通过 |
| Q3 | 🟠 Important | 两端设置试用的“修改查询”只返回编辑阶段，已经确认到历史中的关键词无法删除；删空后仍因历史非空阻止切换方式 | 采纳；双端增加“查询退格”，重用核心按字素退格动作；实际查询为空时允许切换并清除旧方式的空历史。见双端 `ExpressionTrial` 实现 | 共享试用回放覆盖组词、字面两种方式的查找→修改→退格→删空，查询不提交宿主；Kotlin 编译通过 |
| Q4 | 🟠 Important | 保存非法日期规则虽然拒绝写入并报错，但不自动定位出错分组/字段，长列表或其他页保存时难以修复 | 采纳；Windows 保存前使用共享校验，切到特殊输入并聚焦/滚动到字段；Android 保存错误定位到对应规则，字段申请焦点/滚动。尚不能反序列化的空/非整数位置在桥接前定位，Rust 仍执行最终结构校验。见 [save.rs](../../../../windows/settings/src/save.rs)、[SettingsModel.kt](../../../../android/app/src/main/java/org/mywubi/SettingsModel.kt)、[ShortcutSettings.kt](../../../../android/app/src/main/java/org/mywubi/ShortcutSettings.kt) | Windows 三类字段的无窗口保存/焦点测试通过且无配置写入；Android 增补空位置、重复位置的定位断言并编译，设备执行待验 |
| Q5 | 🟡 Minor | Windows 窄窗特殊输入详情缺少设计中约定的 Esc 返回层级 | 采纳；没有弹层时 Esc 返回列表并保留草稿，有弹层时不穿透返回。见 [special_input.rs](../../../../windows/settings/src/panels/special_input.rs) | 无窗口键盘事件覆盖弹层保护、返回和草稿保留 |

未扩大修复的边界：Windows Legacy 主输入的确定未写入失败按原契约已经清理查询，不应为了复用时间而恢复候选。新增测试最初把此分支误认为应保留快照；核对 `acknowledge` 与 SI 规则后修正测试，保留现有清理行为。普通输入的预测热切换也有既有测试约束，本次仅隔离动态时间相关状态。

### 复查与检查结果

- 需求复查覆盖：六类资源身份/开关/错误隔离；大小写与冲突暂停；格式与限值；普通/特殊/动态同文规则、全局位置及分页；完整命中取时、失败/重试/重载；学习隔离；双端草稿、预览、导入与备份。修复后复查差异，未再发现本次范围内可确认的代码问题。
- `cargo test --offline --workspace --lib --tests --jobs 2`：60 个测试程序，**504 项通过、6 项既有条件测试忽略、0 失败**。日志为 `target/validation/special-input/review-workspace-tests.log`，SHA-256 `d54e4c63b5b7d903621bc0bd9c5ca5191002d6605ad561a865e581918018cca4`。其中 settings 29 项、快捷输入 17 项、查询 16 项、预测会话 20 项均通过。
- 核心库严格 Clippy 通过；settings 库 Clippy 完成，仍有 6 项既有告警，位于 `app.rs`、`model_catalog.rs`、`panels/dictionary.rs`，本次新增/修改的 SI 实现无告警。未关闭 lint，也不宣称整个工作区严格 Clippy 通过。
- Android 主程序及设备用例 Kotlin 离线编译通过（`-x cargoBuild`），ARM64/x86_64 的 `mywubi_android` 原生目标另经离线 `cargo check` 通过。首次直接检查未设置 NDK 编译器，误用系统 clang 导致 `ring` 缺少 `assert.h`；显式指定本地 NDK 27.2 的目标 clang/llvm-ar 后两目标成功，无源码绕过或下载。这些检查不是设备测试执行，不能证明焦点、软键盘、TalkBack 或实际上屏已通过。
- 冻结 `cases.json`、`workloads.json` 的 SHA-256 与 SI-01 一致，未改验收期望或性能门槛。此次不重做性能测量，前述数据仍对应先前制品；共享核心修复位于运行时切换分支。
- 语句/分支覆盖率未得到有效数值：本地没有 `cargo-llvm-cov` 和 Rust `llvm-tools`，系统 LLVM 17 与当前 Rust 的 LLVM 23.1.1 不匹配；未安装新工具或把定向测试通过率写成覆盖率。故不能宣称达到审核技能默认的 90% 覆盖率要求，也没有逐函数百分比。

结论：已确认的 5 类问题已修复，后台回归通过，可以提交本次修复。SI-05、SI-10～SI-17 的已有真实环境/设备性能缺口继续保留，不能将此审核等同于系列全部完成；本次不修改 ROADMAP 勾选状态。旧 Setup/Portable/APK 不含本次补丁，下一次实机验收前需从修复后提交重新构建，继续复用本地缓存。
