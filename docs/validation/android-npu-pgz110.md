# PGZ110 预测设备验收记录

2026-11-26，PGZ110 为 Android 15（API 35），SoC 为 MT6895Z_A/TCZA，首选 ABI 为 arm64-v8a。系统构建为 `OnePlus/PGZ110/OP5911:15/AP3A.240617.008/S.1d29cb5-f8-111be:user/release-keys`。11 月 23 日的基线记录来自 Android 14，不能作为这次同系统 CPU 对照。[历史 CPU 测量](android-pgz110-20261123/final-physical-model-duration.jsonl)

| 验收项 | 已取证 | 待验 |
| --- | --- | --- |
| 检测与回退 | adb 的系统服务清单包含 `mtk-dsp_shim`、`mtk-gpu_shim`、`mtk-mdla_shim` 和 `mtk-neuron_shim`。桌面真实 ONNX 图通过 CPU 固定样例；定向测试覆盖无后端、预算变化、连续超时和延迟重试。 | 普通应用枚举、设置显示与复制、独立宿主的 CPU 回退提交及失焦。 |
| NPU 实际执行 | 运行库与模型摘要已核实，已生成读取节点分配的验收包。 | PGZ110 目标模型尚无应用内加载、预测或分配记录，执行位置未验证。 |
| 端到端收益 | 已准备相同模型、词典规则和 CPU 对照。 | 首次加载、键入到候选可见的中位数与 P95、超时率、有效候选和候选质量均无本轮真机结论。 |

使用独立应用 `org.mywubi.npuaccept`，系统在首次安装时进入 `com.android.settings.password.ConfirmLockPassword`。截至此次记录，包管理器尚未安装该应用，系统输入法仍为 `com.baidu.input_oppo/.ImeService`。已通知机主在手机上确认安装。

## 构建与回归

- `cargo test --workspace --lib --tests --locked --offline` 通过 627 项，8 项按原条件忽略。
- `cargo test --offline -p model_input --features android-nnapi --lib` 通过 44 项，含真实 ONNX CPU 预测、权重和策略校验、同机门槛、回退与会话重建。Windows ORT DLL 从已有 CPU 组件包解出，摘要与仓库记录一致。
- ARM64 和 x86_64 原生后端均完成构建。独立 ARM64 APK 及 instrumentation APK 构建通过，Android JVM 测试通过 23 项。真实宿主测试需在手机确认安装之后运行。
- 本轮组件、模型和构建依赖均从本地复用，Gradle 和 Cargo 使用离线模式。

| 本地制品 | SHA-256 |
| --- | --- |
| 验收 APK | `d2a195617f4f6633f61a1ea3a98c5a2a31e77702ba63d7b0f0be63d8c4745213` |
| instrumentation APK | `1f6f61fba044d0d26693ea17fcdfdfa218da62a9b4947c293c16fb8aa53d7799` |
| ONNX Runtime 1.31.1 ARM64，官方缓存 AAR 内的原始 SO | `46e0c16cd515c62a1dd6c5994448b5413705478354bc20a756a5a82634c3ca03` |
| counts.onnx（FP32 候选评分） | `4e31a21f7cd0077c0e6de446167524150c933cfe599ee3526f87812ca7db50e5` |
| vocabulary.json | `992bd5f124d6b1b4c1332fa8fbb5657fcc7a1d3c2714f214019f74398fb4cd05` |
| Qwen2.5 GGUF 生成模型 | `74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db` |

## 真机验收顺序

1. 核对已装验收 APK 的版本与摘要，复用手机现有的独立原生编辑器。将本地 ARM64 SO 和 GGUF 生成包放入验收应用专用的外部 files 目录。
2. 运行 `NpuHardwareTest#fixedModelsAndSystemDrivers`。先核对缺组件状态，再加载哈希匹配的组件，用相同固定前文比较候选；分别取出设备类型、驱动、模型加载时间、CPU／NNAPI 样例结果和运行库节点事件。保存 `files/npu-validation.json`。
3. 运行 `NpuHardwareTest#settingsShowFixedSampleAndPrivateDiagnostics`。从正常预测设置入口切换自动及仅 CPU，重新检测并实际复制诊断信息。保存 `files/npu-settings.txt`。
4. 使用 `prediction_host=editor` 运行 `PredictionHostTest#modelScoredPredictionsUseTheConfirmedHostCommitPath` 以及 `#automaticDeviceFallbackCommitsAndDiscardsStalePredictions`。后者只注入“加速器不可用”，使用真实 CPU 模型和宿主编辑框，检查正文、退格、离开和返回；保存 `files/npu-host.json`。若确认 NPU 真有分配，再进行同机屏幕时延对照。
5. 恢复原输入法、旋转设置并退出测试宿主和验收应用。仅清理本轮创建的独立验收包与测试数据。
