# SK Windows 性能与多宿主续验

2026-11-14。复用已有字体、经典记事本、Edge、码表、图片包与 Cargo 缓存；禁止新增下载。开始测量前固定以下方法，原 SK-01 门槛不放宽。

## 固定方法

- 原 `skin_baseline` Release 命令、7 个状态、4 档 DPI、20 次预热及 200 次样本保持不变，当前制品重复三轮；与已记录的原生排版/光栅数据比较，不能解释为 TSF 可见延迟。
- `skin_probe measure` 增加实际候选场景的旧外观、三套参数明暗和竹纸明暗，96 DPI，每状态 20 次预热和 200 次计时。单独记录加载/解码、热渲染、1000 次图片加载/销毁曲线。
- 在一个离线 Sandbox 中注册最终 DLL，使用经典记事本与 Edge 的 textarea/contenteditable；另开记事本进程用于进程隔离。主机 TIP 不做注册、卸载或替换。
- 每次配置写入都有唯一颜色标记；DLL 日志记录发布进程 PID 和标记，收到全部目标进程确认后才计入一次成功切换。20 次预热后执行 1000 次，10 次一采样，记录私有内存、工作集、GDI/USER 对象及句柄，最后静置 10 秒。第 500 次暂停并切换前台宿主，随后继续；曲线保留原始数据。
- 配置保存到全部加载线程确认的时间只称后台换肤时间，不称逐键响应或呈现时间。既定资源门槛仍是稳定平台、无随次数增长的泄漏；不在结果出来后添加放宽的阈值。
- 同一已有 API 34 x86_64 模拟器另做 Android ART/JNI 双变体加载与位图回收：20 次预热、1000 次加载，每 25 次显式 GC 后采样 PSS/私有脏页/原生堆和旧位图弱引用；不测 UI 帧，不当作物理设备性能。该测试与 Windows 计时分开执行，避免编译或另一性能测试争用 CPU。
- 可见延迟原门槛仍为同宿主 200 次采样、共同时间基准、p95 增量 ≤2 ms。截图轮询和 API 返回时间不具有实际合成呈现时间，若当前工具不能建立该证据则保留未测。

## 一次集中界面清单

1. 记事本输入 a，核对 3 个候选、数字 2 上屏「式」；g/空格上屏「一」，回车原文、取消和退格。
2. Edge textarea 与 contenteditable 执行同一序列；候选不抢焦点、宿主切换清除旧编码。
3. 编码中浅深/参数与图片切换，核对候选保持和选词；移动到屏幕边缘，检查避让和点击。
4. 三进程千次换肤、每进程确认与资源曲线；切换期间保持一个可见候选，检查结束后上屏及继续输入。
5. 在同一 Sandbox 验证配置便携与 AppData 回退，记录实际加载路径。不能把路径测试等同完整安装升级验收。
6. 完成后关闭所有测试宿主与 Sandbox，再整理证据。真机、TalkBack、AK-17 联合矩阵继续未完成。

## 结果

### Windows 原生后台测量

原生基线三轮均为同机 Release、离线字体、每格 20 次预热和 200 次样本。下表为排版与光栅 p95，单位 ms；完整 7 状态 × 4 DPI 数据保存在 [第一轮](validation/skins/performance/native-baseline-1.json)、[第二轮](validation/skins/performance/native-baseline-2.json)、[第三轮](validation/skins/performance/native-baseline-3.json)。

| 状态 / DPI | 原记录 | 本轮 1 | 本轮 2 | 本轮 3 |
| --- | ---: | ---: | ---: | ---: |
| 普通 / 96 | 0.570 | 0.588 | 0.504 | 0.766 |
| 普通 / 192 | 0.498 | 0.667 | 0.534 | 0.566 |
| 长词 / 96 | 0.574 | 0.757 | 0.870 | 0.899 |
| 长词 / 192 | 0.979 | 0.985 | 1.030 | 1.003 |
| 表情 / 96 | 4.359 | 4.177 | 3.858 | 4.017 |
| 表情 / 192 | 4.013 | 3.304 | 4.080 | 3.412 |

另一个候选场景探针复用实际 `preview::candidate` 绘制路径，每格包含固定编码与一个状态样例候选，96 DPI。普通状态 p95：旧外观 0.912 ms、jade 浅色 1.553 ms、竹纸浅色 3.692 ms；表情状态分别为 4.475、5.235、7.028 ms。图片增加了后台绘制成本，不能据此认定实际呈现 p95 增量 ≤2 ms 已通过。[全部明暗方案及资源曲线](validation/skins/performance/native-skin-measure.json)

1000 次图片加载、解码、绘制和销毁的 p50/p95/p99 为 28.459/35.035/38.920 ms；私有内存从预热后 20.06 MiB 回到 19.22 MiB，GDI/USER/句柄分别保持 2/1/193。原始曲线每 10 次采样；这不是 UI 逐键测试。

### Windows 三进程与真实输入

复用一个离线 Sandbox，注册最终 Release DLL 到 `C:\SkinPerfApp2`。两个经典记事本和 Edge 主进程实际加载的 DLL SHA-256 为 `9c6f37820a1c1b267e7659703819672019706443a8e0daacb4e4c1a8e1a3ecb6`，与本地最终产物一致。主机 TIP 未改动。

- 记事本、Edge textarea 与 contenteditable 均验证实际编码、候选和上屏。数字、空格和鼠标选词取得对应「式」「一」结果；记事本保存文件也核对了实际 Unicode 文本。正文使用 Microsoft YaHei，候选长文本和表情使用原生回退；未把这个字体选择推导为所有宿主字体矩阵通过。
- 本轮发现并修复两处问题：TSF 的零 COLORREF 被记事本解释成黑色预编辑背景，改为继承宿主颜色；关闭辅助功能时首次按键未绑定编辑上下文，首次点击才绑定导致候选过期，现由按键入口无条件绑定。后者定向测试先失败（期望「工」，实际空字符串），修复后通过。`im_engine` 最终 82 项单元与 3 项集成测试通过，1 项既有窗口测试保持 ignored。
- 右下角候选框向左、向上避让。第 500 次换肤后在该位置点击第二项上屏「式」，保存文件正确；再切到 Edge，保持编码执行后半程，第 1000 次后点击第二项仍上屏「式」。点击不夺走宿主输入焦点。[右下角中点](validation/skins/performance/final-stress-500-right-bottom.jpg)、[结束后选词](validation/skins/performance/final-stress-after-click.jpg)
- 便携配置与 AppData 回退均实际运行：备份 `C:\SkinPerfApp2\config.toml`，把已有配置、码表和皮肤复制到 `C:\Users\WDAGUtilityAccount\AppData\Roaming\MyWubi`；新宿主使用该处深色配置，输入上屏正确。最终千次测试也只修改 AppData 配置。此项不等于完整安装、升级或权限矩阵通过。[AppData 深色图片](validation/skins/performance/notepad-appdata-dark.jpg)

最终三进程各确认 20 次预热及 1000 次正式切换，后台发布 p50/p95/p99 为 **45.944/54.753/59.096 ms**。每次包含保存、文件通知、加载/解码和三个发布日志的轮询确认；不含真实合成呈现时间。三个进程的计数与私有内存反复回落，未见随切换次数持续增长。[完整曲线与静置采样](validation/skins/performance/windows-stress-final.json)、[资源汇总](validation/skins/performance/resource-summary.json)、[日志中每进程 1020 个唯一标记核对](validation/skins/performance/acknowledgements.json)

补充现场截图：[修复后预编辑](validation/skins/performance/notepad-fixed-preedit.jpg)、[记事本首次鼠标选词](validation/skins/performance/notepad-first-click-fixed.jpg)、[两种浏览器编辑区](validation/skins/performance/edge-final-two-editors.jpg)。制品、PID、路径及资源复用记录见 [测量来源](validation/skins/performance/provenance.json)。

| 最终宿主 | PID | 切换期私有内存范围 MiB | 静置后 MiB | 切换期 GDI 范围 | 静置后 GDI |
| --- | ---: | ---: | ---: | ---: | ---: |
| 记事本 B，前半程可见 | 6728 | 24.770–36.277 | 34.184 | 24–33 | 24 |
| 记事本 A，后台 | 6124 | 30.898–45.152 | 32.887 | 24–24 | 24 |
| Edge，后半程可见 | 6284 | 68.887–92.398 | 67.004 | 23–29 | 23 |

保留 [首次压力测量](validation/skins/performance/windows-stress.json)，其后台 p95 为 68.082 ms；该轮尚未修复首次鼠标选择，不用于宣称点击闭环通过。另一次工具试运行因将颜色写成整数而被配置校验拒绝，已改用契约要求的颜色字符串，该次没有计入任何成功样本。

### Android 模拟器资源回收

复用已有 Pixel 5 API 34 x86_64 AVD、已安装主 APK、缓存 JNI 和实际编辑器导出的图片包；只替换新增测试 APK。`SkinPerformanceTest` 在 ART 中完成 20 次预热和 1000 次双变体加载，每次断言浅深变体共享同一位图，设备测试通过（263.168 秒）。采样按含预热的累计次数每 25 次执行，正式次数为 0、5、30、55…980，末尾另采样 1000；保留实际偏移，没有改写成 0、25…1000。

加载/解析/JNI/图片解码 p50/p95/p99 为 **225.372/431.968/581.913 ms**。所有资源采样点旧位图弱引用存活数为 **0**；PSS 74339→74431 KiB，私有脏页 64740→64884 KiB，原生分配 11181392→11200448 bytes。统计数组和弱引用记录本身也随样本增加；没有出现退休图片仍存活或图片体量随次数累积的现象。[设备原始结果](validation/skins/performance/android-loader.json)

这些是模拟器的后台换肤与 GC 后资源数据，不是触摸响应、UI 帧耗时、自然 GC 压力或物理 ARM64 设备结论。

### 覆盖边界与复现

两轮 Windows 计时没有与 Android 性能测试或编译同时运行，Android 计时也没有与编译重叠。复用所有本地依赖，没有下载；完成后 `wsb list` 为空，`adb devices` 无在线设备。

最终 DLL 的鼠标选择修复已有现场复测；图形资源长测通过使 SK-16 可标完成。SK-01 的实际可见延迟、SK-11 的完整编辑器/安装/多 DPI/多屏矩阵、SK-14/17 的真机与 TalkBack、SK-19 完整回归及依赖 AK-17 的 SK-20 仍未完成。当前 Sandbox 的 `ms-settings:display` 没有进入显示页，系统设置也无显示入口，未修改主机缩放来补齐矩阵。[环境证据](validation/skins/performance/sandbox-display-settings-unavailable.jpg)

复现命令：

```text
cargo run -p text_render --example skin_baseline --release --offline --locked
cargo run -p text_render --example skin_probe --release --offline --locked -- measure CONFIG OUTPUT.json
skin_probe.exe stress CONFIG DLL_LOG OUTPUT.json PID1 PID2 PID3
adb shell am instrument -w -e class org.mywubi.SkinPerformanceTest org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
```

`stress` 使用新输出文件名；第 500 次写入同名 `.pause` 后，完成前台宿主切换再创建 `.resume`。结束或可返回的错误路径会恢复原配置。测试进程须先实际激活 TIP，PID 须来自当前实例。Sandbox 命令遵循 [Microsoft CLI 文档](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-cli)，未调整执行策略。
