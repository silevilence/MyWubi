# PGZ110 预测设备验收记录

2026-11-26，一加 Ace 竞速版 PGZ110，Android 15（API 35），SoC MT6895Z_A/TCZA，arm64-v8a，ONNX Runtime 1.31.1。系统构建号及普通应用读取的驱动版本已保存。[真机数据](android-npu-pgz110.json)

| 验收项 | 结果 | 依据 |
| --- | --- | --- |
| 应用检测与 CPU 回退 | 通过 | 缺 ONNX 组件时应用能枚举硬件，安装组件后能查询 NNAPI 后端。设置页完成设备选择、重新检测、固定样例和诊断复制。独立原生编辑框验证了预测上屏、退格和失焦。 |
| 目标模型的 NPU 执行 | 未验证 | counts.onnx 在 CPU 上完成评分，但创建 NNAPI 会话返回 `NNAPI_LOAD: unsupported model format, architecture or operator`，未取得节点分配记录。现有 GGUF 生成包使用 llama.cpp CPU。 |
| NPU 端到端收益 | 未验证 | 两个目标模型均无可供比较的 NPU 预测结果。自动模式保留 CPU，本轮没有 NPU 按键到候选可见时延或加速倍数。 |

## 设备与固定样例

普通应用通过 NNAPI 枚举到 `mtk-dsp_shim`、`mtk-mdla_shim`、`mtk-neuron_shim` 三个专用机器学习加速设备（类型 4），`mtk-gpu_shim` 为 GPU（类型 3），四个驱动均报告 5.1.1。系统另有 CPU 参考实现 `nnapi-reference`。查询、加载和实际出词分别取证。

在同一台手机、两线程、1000 毫秒预算下，固定前文按“北京、天气、明天去”重复两轮。计时覆盖样例调用，不含键盘显示。

| 模型与任务 | 加载时间 | 最终六次 CPU 样例 | 候选举例 |
| --- | ---: | --- | --- |
| counts.onnx，候选评分 | 10.2 ms | 6/6 有候选，耗时 20.4～23.6 ms | “北京”前两项为“天气、大学” |
| Qwen2.5 GGUF，下一词生成 | 2.57 s | 5/6 有候选，五次成功耗时 394～456 ms | “北京”→“市”，“天气”→“晴朗”，“明天去”→“出差” |

GGUF 最终测量的第一轮超时，从开始到返回用了 76.1 秒。较早的一次运行也有两轮超时。样例在 instrumentation 进程中启动，最终测量期间将设置切至前台；这些数据不能用来估算日常键盘的首词时延。已保留两次原始数据。11 月 23 日的历史 CPU 基线使用 Android 14；本轮对照使用 Android 15 上实测的 CPU 数据。[历史记录](android-pgz110-20261123/final-physical-model-duration.jsonl)

## 实际输入与复测

- `PredictionHostTest#modelScoredPredictionsUseTheConfirmedHostCommitPath` 通过。在独立宿主选中“北”再接受预测，正文变为“北京天气”；新编码和外部编辑清除旧候选。
- `PredictionHostTest#automaticDeviceFallbackCommitsAndDiscardsStalePredictions` 使用独立宿主（`prediction_host=editor`）。注入“加速器不可用”时，实际 CPU worker 完成上屏并记录 `NNAPI_UNAVAILABLE`。首次启动宿主未成功唤起键盘，回到桌面并重新选择输入法后同一用例通过。
- 使用真实检测结果再次运行回退用例，键盘记录为 `CPU / NNAPI_LOAD`。选词得到“北京天气”，退格得到“北京天”；离开再返回继续输入得到“北京天北”。随后打开第二个独立编辑框，核对初始正文为空、旧预测消失，重新输入后上屏“北”。
- 真机首次在外部目录读 GGUF 包遇到权限错误，改为应用私有测试目录并核对同一份权重的 SHA-256。没有重新下载模型。另修复了 NNAPI 加载失败后仍把 CPU 样例标为“执行设备未确认”的显示问题。
- `NpuHardwareTest#fixedModelsAndSystemDrivers` 验证组件、图评分及 GGUF 生成；`#settingsShowFixedSampleAndPrivateDiagnostics` 从正常设置入口操作并读回诊断内容。

- `PredictionHostTest#generatedDictionaryExternalWordsAppendInTheRealHost` 使用相同 GGUF、独立原生编辑器和两线程，在“图书”后先选“作者”、再选“出版社”，从宿主读回“图书作者出版社”。
- 运行命令为 `adb shell am instrument -w -r -e prediction_host editor -e class org.mywubi.<测试类>#<方法> org.mywubi.npuaccept.test/androidx.test.runner.AndroidJUnitRunner`。真实 NNAPI 失败分支额外传入 `-e npu_real_fallback true`；GGUF 连续生成传入 `-e generation_chain books`。
- 已恢复原输入法 `com.baidu.input_oppo/.ImeService`、自动旋转与原方向，退出验收宿主并卸载两个独立验收包。核对包清单、进程和外部测试目录后，手机回到原桌面。

## 构建和制品

Rust 工作区离线测试通过 627 项，8 项按原条件忽略。启用 Android NNAPI 功能的模型专项通过 44 项，Android JVM 测试通过 23 项。ARM64 与 x86_64 原生后端均完成构建，手机安装独立包 `org.mywubi.npuaccept`。APK、组件、模型和 Gradle/Cargo 依赖均从本地复用。

| 制品 | SHA-256 |
| --- | --- |
| 最终 ARM64 验收 APK | `6b2a1bc3af2188a9e2ada8b54097c537e2888b5f0b8279f632d3120282312362` |
| 最终独立编辑框 instrumentation APK | `2c657eb72dadfebf302136926809dbb854bc7e297eb8ecf9c0272dce9eb568f9` |
| ONNX Runtime 1.31.1 ARM64，缓存官方 AAR 内的 SO | `46e0c16cd515c62a1dd6c5994448b5413705478354bc20a756a5a82634c3ca03` |
| counts.onnx | `4e31a21f7cd0077c0e6de446167524150c933cfe599ee3526f87812ca7db50e5` |
| vocabulary.json | `992bd5f124d6b1b4c1332fa8fbb5657fcc7a1d3c2714f214019f74398fb4cd05` |
| Qwen2.5 GGUF | `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db` |
