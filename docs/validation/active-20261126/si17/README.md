# SI-17 安卓特殊输入与导航 · 2026-11-26

在 Android 15 的 PGZ110 使用独立编辑器回读提交文本。设备保留日常安装的 MyWubi，测试安装包使用单独的应用 ID。两个包中的 ARM64 核心库具有相同的 SHA-256。固定协议、跨日及失败重试的设备样例见 [SI-15](../si15/README.md)。

在手机上点选 C03～C07 的候选，检查同文不同来源、候选容量 1/9/10 和全局位置 9/10/11 的九个组合，以及 C11 前缀能否继续输入。两种查询方式、特殊符号和日期时间的选择、取消及返回均从独立编辑框回读；C08/C09 的精确跨日和失败重试另由真机固定时钟用例覆盖。[真机方法结果](phone.json) · [落位逐项记录](special-positions.jsonl) · [跨端样例和固定时钟](../si15/README.md)

复测中有两次启动独立宿主时系统没有绑定输入法。另一次从切换压力测试留下的 14 键布局启动，导致测试找不到 26 键布局的 `j`。恢复测试所需的初始布局后，两组候选落位测试均在真机通过。测试代码现会暂存原布局，结束时按原字节还原。最终测试 APK 已构建，并直接调用其中的 Kotlin 文件处理逻辑验证了无原布局、已有分组布局和测试异常三条还原路径。五项特殊输入宿主测试也在网格优化后的构建上全部通过。

API 34 x86_64 模拟器通过 20 个独立用例。宽屏设为 1600 × 2100 像素、240 dpi，系统字号 1.6 倍、深色主题；另用 Compose 明确指定普通字号和两倍字号，覆盖横竖屏候选展开、滚动、选择、取消和返回。普通竖屏的测试曾把「取消」文字的 30 像素字高误作触控区域。修正后用其可点击父按钮的真实边界复核通过。[模拟器逐项结果](emulator.json)

在 Android 手机和模拟器上分别核对 MyWubi 的静默可访问性节点。TalkBack 实际语音、朗读顺序和专属手势沿用用户已指定的免测范围。[既有决定](../../android-pgz110-20261123/README.md)

### 输入到候选真正显示

在独立编辑框中输入 `p`，以输入法窗口收到触摸按下的系统事件时间为起点。按下和抬手时间精确到毫秒。触点约停留 40 ms，抬手后保留正常的手势判定。每次触摸前先确认候选「这」未出现；在输入法第一次绘制含该候选的布局时记录时间，通过 HWUI 找到对应的提交帧，再用 GPU 完成围栏从 SurfaceFlinger 的输入法图层历史中精确找到该帧的屏幕呈现时间。每次另外比对候选像素，并点选后从外部编辑框读回「这」。振动开关也通过了系统记录复核。

三轮各预热 5 次、测量 60 次。下表和验收门槛都从按下开始计算。同步记录的抬手、绘制与上屏时间可用于定位开销。

| 轮次 | 按下到显示 P95 / P99 | 抬手到显示 P95 | 手指停留 P95 |
|---|---:|---:|---:|
| 1 | 163.6 / 166.7 ms | 113.9 ms | 52.0 ms |
| 2 | 140.6 / 151.0 ms | 93.6 ms | 51.0 ms |
| 3 | 136.8 / 140.2 ms | 89.6 ms | 51.0 ms |

特殊输入要求每轮 P95 ≤200 ms、P99 ≤300 ms，本次三轮均达标。键盘 AK-17 另有 P95 ≤50 ms 的门槛。首次使用相同测量方法时，优化前的三轮 P95 为 221.3、186.5、162.3 ms。

195 次按下（含预热）都匹配了唯一的 GPU 围栏，并逐帧与 Perfetto 的输入法表面及物理屏幕时间线交叉核对，两种呈现时间相差最多 1.8 微秒。处理器报告设备轨迹中有负时间戳事件被丢弃，195 个目标帧均可双向核对。截图单独放在绘制后，以免系统截屏把输入延迟放大。[原始帧及触摸时间](physical-input.json) · [分布、逐帧结果、制品及轨迹摘要](presentation.json) · [Perfetto FrameTimeline](https://perfetto.dev/docs/data-sources/frametimeline) · [AOSP FrameTracker](https://android.googlesource.com/platform/frameworks/native/+/master/services/surfaceflinger/FrameTracker.cpp)

在真机启用 `physical_acceptance=true` 运行 `PhysicalPresentationTest`。并行记录系统 Perfetto 数据源 `android.surfaceflinger.frametimeline`，用本次原始轨迹复算的命令如下。

```powershell
python scripts/validation/candidate_presentation.py docs/validation/active-20261126/si17/physical-input.json target/active-device-20261126/shift-first-frame.pftrace target/active-device-20261126/trace_processor_shell.exe
```
