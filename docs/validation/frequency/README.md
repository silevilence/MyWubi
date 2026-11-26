# 词频统计与码表调整验收

2026-11-26。Windows 使用离线 Sandbox 内的 WPF TextBox 和真实 TSF 输入法。Android 使用 API 34 x86_64 模拟器内独立 APK 的原生 EditText，通过 MyWubi 输入法服务与 JNI 上屏。两端使用同一份样例，每种纠正方式换用独立的码表路径，从零计数。

## 纠正误选

编码 `ab` 包含「以及」和「已经」，基础分分别为 10 和 9。对照组只选一次「已经」。其余场景先选「以及」，核对正文及次数已经增加，再纠正并选「已经」。

| 场景 | 最终正文 | 焦点和选区（UTF-16） | 「以及」净增 | 「已经」净增 |
| --- | --- | --- | ---: | ---: |
| 只选一次正确候选 | 已经 | 原编辑框，2–2 | 0 | 1 |
| 退格两次，中间正文为「以」 | 已经 | 原编辑框，2–2 | 0 | 1 |
| 宿主撤销整次上屏 | 已经 | 原编辑框，2–2 | 0 | 1 |
| 选中整词，直接输入正确候选替换 | 已经 | 原编辑框，2–2 | 0 | 1 |

每例从统计文件读取两词的次数，并重新输入同一编码查看候选。在此样例中，「已经」增加 1 分后与「以及」同分，沿用原顺序。四例的预览都只有「已经」一条，显示由第 2 位到第 2 位、增加 1 分。另用分差为 10 的核心用例验证多次选词可以升位，扣回一次可以降回原位。

Windows 双宿主使用两个独立进程。两边各选一次「以及」后合计为 2，第二个编辑框逐字删空后合计为 1，第一个编辑框仍保留「以及」。

## 数据与排序

- 同码同词的系统分 100 与用户调整 −30 合并后为 70。负分自造词仍可见，同词不同码单独排序。检查系统码表原文以及用户词典与系统码表误指同一文件的情形。
- 旧词典按当前码表换算，负调整和自造词可导出再导入。手动调位覆盖同分、相邻分值、更新已有条目以及过期候选身份。
- 自动统计与候选加权可以分别开关。手动写入测试覆盖已有 +5 加上未写入的 +12 后为 +17、预览过期、并发编辑冲突和重复确认。
- 纠错依赖宿主能提供同一编辑框的上屏前后状态。编辑框切换、上下文不完整或无法确认是哪次上屏时，保留已有计数。宿主自身的撤销粒度由编辑器决定。

## 交互与构建记录

Windows 实测右键取消后保留预编辑及焦点，点击「移到第 1 位」后首选立即刷新，按 1 上屏「已经」。菜单打开时点击另一个输入框，原框保留「已经ab」（WPF 失焦时保留预编辑文本），新框为空，用户词典哈希未变。[Windows 逐例结果](windows.json)

Android 分别使用默认的宿主预编辑和「仅在候选栏显示预编辑」完成以上四例。两种模式都验证了候选栏长按、系统返回键取消、展开列表长按调至首位、空格按新名次上屏，以及菜单打开后切换编辑框。新框为空，旧菜单消失，用户词典内容未变。[Android 两种预编辑模式的记录](android.json)

- Rust 工作区通过 641 项，失败 0 项，8 项按仓库配置忽略。同码候选第二页的最后一项移至第 1、2、3 位也已单独复测。
- Android 本地单元测试通过 23 项。两个设备测试方法均通过，分别覆盖两种预编辑模式。Windows Sandbox（10.0.26100.8655）通过 6 个场景。Android 使用本地 API 34 模拟器，本轮未在实体手机上复测。
- 首轮发现 Android 非聚焦菜单按系统返回键会收起键盘，切换输入法时还可能向已关闭的加载线程提交任务。修复后重跑两种预编辑模式。Windows 起初存在第二宿主启动、窗口焦点和菜单选择的脚本问题，改用显式宿主焦点及真实菜单项点击后，在同一沙盒复测通过。
- 一次并行构建期间，Windows 文档测试未能加载本地构建的渲染库。完成 DLL 构建后独立重跑整个工作区通过。原始构建与测试日志位于本地 `target/frequency-acceptance/`，包括 `workspace-delivery.log`、`android-lifecycle.log` 和 `sandbox/results/`。
- 本轮复用了本地 SDK、模拟器镜像、Cargo 和 Gradle 缓存。测试结束后已恢复模拟器输入法，关闭模拟器及 Windows Sandbox。

| 实际验收制品 | SHA-256 |
| --- | --- |
| Windows 输入法 DLL | `9329EBC260ABD41CAE5EAA4A04B48F8E977C3825EC483906741539B2AC21DB97` |
| Windows 预览探针使用的核心 DLL | `EA8D7B9DE9687FA5EB2F3E2787E38C752C3F4E36818BA31CA137E233FCB250A2` |
| Android 输入法 APK | `273C3E50E732EE490B9F4D0404F52044CE3E86AC6446294FEB9B2A0FE250C4F7` |
| Android 测试 APK | `9EED665082F4B7377FA5F42CDD630FFC1AADB948BDC5C4873F2BFD5DDD5D6770` |
| Android 外部宿主 APK | `EC91C9E0EC7D3FC5835B0EB814C908AB26850DD229E25925A622022A52EC0B49` |

## 复现

在仓库根目录运行 `cargo test --offline --workspace`。Windows 用 `cargo build --offline -p im_engine -p core_engine` 构建，再运行 `.\scripts\test-windows-frequency.ps1`。脚本输出本次证据目录，其中的 `results/frequency.json` 记录逐例结果。

Android 复用本地 SDK、NDK、模拟器镜像与 Gradle 缓存，执行以下 PowerShell 命令。将三个 debug APK 安装到同一测试模拟器后运行设备测试。

```powershell
.\android\build-models.ps1 -Tasks @('--offline','-PacceptanceHost=true',':app:assembleDebug',':app:assembleDebugAndroidTest',':app:testDebugUnitTest',':acceptance-host:assembleDebug')
adb -s emulator-5570 shell am instrument -w -r -e class org.mywubi.FrequencyHostTest org.mywubi.test/androidx.test.runner.AndroidJUnitRunner
```
