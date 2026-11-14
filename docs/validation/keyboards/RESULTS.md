# AK-01～20 集中验收记录

日期：2026-11-14。此记录只对应本次构建和执行，不借用历史截图补齐未运行项。全部码表、SDK/NDK、Gradle/Cargo 缓存、模板与 AVD 均为本地复用；没有下载验证资源。

## 自动检查

- Cargo workspace：588 项通过、0 失败、6 项既有条件测试忽略。原始记录 `workspace-tests.log`。
- 首轮 doctest 在另一构建替换共享产物期间出现 `E0463 text_render`；构建完成后单独运行 `cargo test --workspace --doc --offline` 通过，见 `workspace-doctests.log`。不隐藏首轮错误。
- 收尾模板幂等导入、组合与能力校验：15 项通过；组检索、拼音、会话、快捷输入：17 项通过；核心严格 Clippy 通过。
- 优化后 Windows im_engine 自动回归：82 项通过、1 项既有条件测试忽略，见 `windows-core-regression.log`；最终 DLL/settings 构建通过。
- Android JVM：9 项通过。离线构建包含 ARM64 / x86_64 JNI、主 APK、仪器 APK 与独立 acceptance-host；不把构建成功算作 ARM64 真机运行。

## 模拟器

复用 Pixel 5 / API34 / x86_64 AVD，headless 启动一次。首轮 37 项中，33 项通过、3 项失败、1 项需要显式参数的旧基准跳过；失败原始记录保留在 `android-instrumentation.log`。

三处首轮测试问题修正并定向通过（`android-retest.log`）：方向映射测试在 Compose 重组完成前注入下一次触摸；旧宿主测试查找已启用后不再显示的启用按钮；旧宿主测试依赖 AVD 预存码表。现在等待实际重组，并为宿主测试明确准备/还原真实码表配置。

第二轮暴露模板重复安装错误和另一个布局测试重组期间的旧 UiObject 引用；修复后四项定向测试全部通过（`android-fixes-retest.log`）。综合各轮最后一次结果，37 个功能测试通过、1 个新 JNI 测量脚本运行成功、1 个旧可选基准因未传入参数跳过，见 `instrumentation-summary.json`。测量脚本完成不表示性能门槛已经通过。

已完成的真实独立 EditText 轨迹：十四键 GH/GH/L/L → 王，英文 abc，九键 GHI/GHI/JKL/JKL → 王，九键拼音 MNO/GHI → 你，并返回记忆的形码布局。正文是 `王abc王你`；宿主 TextWatcher 日志 `ak-host-edits.jsonl` 未出现内部字符组标记。截图 `ak14-real-host.png`、`ak9-pinyin-real-host.png` 来自该次运行。

收尾发现并修复：重复添加内置模板因文件存在报错，改用不可变包导入器复用相同内容、拒绝同版本冲突；自定义布局按真实行数分配高度，恢复入口和筛选栏计入预算，空闲时预留筛选栏避免首次组输入移动键位。

更新后的独立宿主轨迹额外断言首次输入前后 GH 键边界不变，并分别以密码、数字、URI、邮箱 inputType 重建宿主，确认保存的九键布局不进入这些输入框；实际文本分别为预期的 q / 1 / q / q。各测试还原原配置和布局记录。

## Windows 编辑器与包往返

实际使用本次便携 settings.exe：创建 jade 副本、切换 Android、添加 shape14、观察几何/组动作/八向继承、点击模拟、校验组件、导出 `windows/editor-shape14.mywubi-skin`。未应用到用户当前输入法配置。连续鼠标拖动未取得可核实的松手结果，因此不宣称该交互验收通过。

Android 仪器测试已导入该实际导出文件、解码十四键、保存组合、完整导出再导入；ZIP 每个条目的 SHA-256 完全相同，含 Windows 参数和共享资源，见 `ak-windows-roundtrip.json`。四种内置模板另有完整包往返测试。系统文件选择器、原生预览、保存及系统分享入口在既有 jade 外观包测试中通过；自定义布局包的设置页应用与系统分享完整操作链尚未独立验收，AK-16 保持待验。

## Windows TSF 限制

Sandbox 复用了当时已构建的本地 DLL、设置程序、码表与映射目录，但默认 PowerShell 执行策略拒绝启动 `.ps1`，见 `windows/startup-failure.txt` 和 `windows/execution-policy.txt`。未改变执行策略，未注册用户主机 TIP；没有有效的最终 DLL 真实 TSF 输入证据。Windows 自动回归和构建不替代这一项。

## 性能与未完成项

主机 release 组检索数据见 `group-baseline.json`、`pinyin-group-baseline.json`，拼音 65,125 条表 p95 为 1.364 ms；仅代表检索。

`ak-jni-benchmark-before.json` 是同 APK、同表的 JNI Prepare/Ack 加候选读取对照，各 500 次、50 次预热。形码完整 ggll 轨迹 p95：精确 6.870 ms / 分组 5.893 ms；拼音完整 ni 轨迹：精确 4.672 ms / 分组 10.131 ms。首轮拼音增量超出 2 ms 门槛，原始失败保留。优化缓存排序键并跳过无追加来源时的整批 Entry 复制后，最终 APK 在无并行构建时复测三轮：

| 轮次 | 形码精确 / 分组 p95（ms） | 拼音精确 / 分组 p95（ms） | 拼音增量（ms） |
| --- | --- | --- | --- |
| 1 | 9.220 / 4.471 | 6.846 / 6.612 | -0.233 |
| 2 | 5.291 / 4.040 | 4.365 / 13.160 | +8.795 |
| 3 | 15.734 / 4.135 | 3.923 / 5.895 | +1.972 |

数据分别为 `ak-jni-benchmark.json`、`ak-jni-benchmark-2.json`、`ak-jni-benchmark-3.json`。第二轮超限且精确路径自身波动明显，不能选择最好一轮宣称通过；JNI 稳定增量门槛保持未完成，需要继续定位抖动并在目标设备复核。这些是完整输入序列的 JNI 耗时，不是单键屏幕可见延迟。PSS 记录包含加载前后差异，没有 30 分钟预热后曲线，不能据此判断长期泄漏或资源门槛。

最终制品摘要见 `android-final-hashes.json`、`windows-final-hashes.json`。最终 APK 的 x86_64 库和主 dex 与功能定向复测制品摘要一致；ARM64 同源码重新构建。测试编辑器、Sandbox 和本次启动的模拟器均已结束，不继续占用用户桌面。

用户明确暂时没有 Android 真机，因此 ARM64 物理设备、TalkBack、真实误触/输入耗时对照、实际可见候选 p95、30 分钟资源曲线保持未完成。完整屏宽/字体/旋转/进程重建与复杂跨包切换矩阵也不能由现有局部测试推断通过。AK-09、AK-16、AK-17 不整体勾选。
