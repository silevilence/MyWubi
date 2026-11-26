# Android 预测模型与加速器

MyWubi 复用随应用提供的 llama.cpp 和可选的 ONNX Runtime 1.31.1。Android 包支持 arm64-v8a、x86_64，最低 API 26。系统 NNAPI 设备枚举和禁用 NNAPI CPU 的验证需要 API 29。Android 15 起 NNAPI 已弃用，系统仍可能提供厂商驱动。设备类型、驱动和模型图共同决定可用性。[Android NDK](https://developer.android.com/ndk/guides/neuralnetworks) · [ONNX Runtime NNAPI EP](https://onnxruntime.ai/docs/execution-providers/NNAPI-ExecutionProvider.html)

| 任务 | 模型包 | 当前 Android 路径 |
| --- | --- | --- |
| 字符图为召回候选评分 | ONNX、character-step-v1、词表、评分策略 | CPU；声明 NNAPI 的包可验证分图和固定样例 |
| GGUF 续写评分或短词生成 | GGUF、支持的分词器、与权重绑定的相应策略 | llama.cpp CPU。该构建没有把 GGUF 解码图交给 NNAPI 的适配器 |
| LiteRT / LiteRT-LM | 需要相应的转换模型、编译器及受支持的 SoC 运行组件 | 当前包没有这些权重或运行组件。不能把旧 GGUF 改后缀当作可加载模型。[LiteRT NPU 指南](https://ai.google.dev/edge/litert/next/npu) |

制作模型包时可用 `package_model.py --android-provider nnapi --quantization fp32` 声明 ONNX 导出；CPU 包使用 `--android-provider cpu` 及实际量化格式。该选项与原有 backend、adapter、capabilities、files[].sha256 一起描述模型。旧包继续使用 CPU。ONNX Runtime 采用 MIT 许可，原包的第三方声明随应用分发。厂商系统驱动由手机提供，应用不再分发一份。模型沿用各自许可。

固定样例使用与输入法相同的词典召回和模型准入策略。节点记录来自开启 NNAPI 并禁用其 CPU 回退的 ONNX 会话。仅有 CPU 节点时报 CPU；NNAPI 节点还要与系统设备类型交叉核对。如果后端也能选择 GPU 或其他未知类型，就不能确定物理执行设备。分布在已确认加速器与 CPU 的节点报告为混合执行。样例诊断只包含固定前文对应的输出与计时。

“自动”先用 CPU。在预测设置的“运行状态与性能”中验证当前模型后，只有全部节点可归到 NPU、六轮固定前文均产出与 CPU 顺序相同的非空候选，且中位和最慢一轮的耗时均比 CPU 缩短超过 15%，才允许该组合用于日常预测。六轮计时从词典召回到候选准入；屏幕出词耗时另在真机编辑框测量。日常 NPU 会话禁止运行库静默把节点放回 CPU。系统、运行库、模型、策略或预算变化后需要重新验证。驱动失败或连续两次超时会暂停该组合；仅 CPU 模式直接使用原适配器。
