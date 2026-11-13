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

生产代码基点 `72d410d`，之后只补充测试和验收记录。Windows 使用缓存的 `target/v080-published/components.json` 设置 `MYWUBI_COMPONENT_CATALOG`，运行 `cargo build --offline --release --locked --jobs 2 -p settings -p im_engine` 成功。三份普通码表及六类特殊资源按共享清单复制；`settings.exe --component-catalog` 仅执行启动前的命令行分支并以隐藏窗口运行，输出与缓存的发布目录一致。

本地 Velopack 1.2.0 生成 `target/si-validation-20261113/packages/` 中的 Setup、Portable 和 nupkg，验收包版本 `0.8.0-si.20261113`，项目版本未更改。使用 `--skipVeloAppCheck true` 跳过需要启动应用的集成探测；该项和安装、升级行为仍待实机验证。没有执行安装器，没有签名或发布。可选模型组件不在此次特殊输入包内，也未重新下载组件归档；只复用已有组件目录元数据，不能据此宣称模型组件交付已验证。

Android 使用本地 JDK 21.0.8、SDK/NDK 和 Gradle/Cargo 缓存，`CARGO_NET_OFFLINE=true`、`CARGO_BUILD_JOBS=2`，执行 `gradlew.bat --offline --max-workers=2 :app:assembleDebug :app:assembleDebugAndroidTest --console=plain` 成功。主 APK 含 ARM64/x86_64 的 Release Rust 库，外壳为 Debug，设备测试 APK 仅编译，未安装执行。既有 SDK package.xml、Gradle 弃用和 Windows 导出链接提示未阻断构建。

`scripts/validation/check-special-input-artifacts.py` 对 Portable、nupkg、APK 内各 22 项资源/许可与源文件逐字节比较；Windows exe/DLL 对照本次 Release 产物。Android 打包会执行符号剥离，直接与 Cargo 输出比较的首次检查失败；确认两种 ABI 的合并输入均等于 Cargo 输出，并用本地 NDK `llvm-strip --strip-unneeded` 独立重放后，与 APK 内文件逐字节一致。此转换链已写入校验工具，不忽略差异。最终检查通过，完整路径、大小及 SHA-256 见 [artifacts.json](artifacts.json)。Setup 只记录容器摘要，不将生成成功写成已安装通过。
