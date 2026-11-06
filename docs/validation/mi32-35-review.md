# MI-32～MI-35 验收记录

日期：2026-11-06。环境、模型哈希和逐次结果保存在 [mi32-35/](mi32-35/)。
用户确认暂无 NVIDIA 与 Android ARM64 设备，相关实测保持待办。

## MI-32 Vulkan

本机 Windows 11 / Core Ultra 7 165U / Intel Graphics 驱动 32.0.101.8425，
Vulkan 实际发现 `Vulkan0`（统一内存）。通过独立 model-worker 完成设备发现、
GGUF 加载、预热、预测、暂停释放与恢复。只覆盖这一个 Intel 驱动/设备组合，
不推断 AMD、NVIDIA、独显、Windows ARM64 或其他驱动兼容。

模型为 Qwen2.5-0.5B-Instruct Q4_K_M，权重 SHA-256
`74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db`；
使用原有 `qwen25-short-words-v1` 适配与准入策略，三个固定公开前文，不读取用户输入。

| 测量 | CPU | Vulkan |
| --- | ---: | ---: |
| 后续进程原生 load | 875 ms | 1064 ms |
| warmup | 656 ms | 262 ms |
| 北京 | 283 ms，市/区/县 | 246 ms，市/区/县 |
| 明天去 | 318 ms，出差/探亲/休假 | 211 ms，出差/探亲 |
| 我想喝 | 65 ms，Empty | 243 ms，水/茶/咖啡 |
| drop | 92 ms | 118 ms |

这是三个样例的适配器对照，不是质量或分位性能结论。输出有差异，不宣称 GPU
更准确，也不因此改变默认 CPU。原始首次驱动运行的 warmup 为 20.6 s、前两次
预测为 5.58/6.48 s；保留在 `vulkan-first-driver-run.jsonl`，没有删掉冷启动成本。
首次运行与生命周期测试时有构建后台负载，因此这些耗时只证明闭环，最终性能
对照须用稳定制品、单独运行再测。

`vulkan-lifecycle.jsonl` 记录独立进程 Ready、首两次超时隐藏、后续 261 ms 内返回、
Paused、恢复 Ready。超时不延长 1000 ms 配置预算、不补发旧结果。
`missing-device.jsonl` 证明请求不存在的设备保留请求信息并在 CPU 成功预测。
自动测试另覆盖设备/后端/显存匹配、可恢复执行失败释放 GPU 后重试 CPU，以及
超时/取消/低置信度不触发回退重算。未强行重置用户显卡驱动来制造真实设备丢失；
原生进程级崩溃不算可恢复 Rust 错误。

构建：固定 llama-cpp-2/sys 0.1.161，Vulkan SDK 1.4.363.1，VS C++ + Ninja；
纯 CPU workspace check、120 项核心/模型单元测试、相关严格 Clippy、Android
`:app:compileDebugKotlin` 通过。SDK 的普通安装因需要提权而回滚，随后按官方支持
的 copy_only 模式将开发文件复制到工作区，没有修改系统权限或安装显卡驱动。
MSBuild ExternalProject 步骤乱序问题通过构建脚本使用 Ninja 解决。

可选组件经 `stage_gpu_component.ps1` 和
`python scripts/check_model_artifacts.py --gpu-dir target/mi32-component` 验证：PE x64、
运行特性 Vulkan/llama/ORT、26 个文件哈希/大小、10 份许可证哈希、无模型权重。
终端用户不需 SDK；Vulkan loader/驱动由系统与厂商提供。独立 CPU 组件继续可构建。
自动组件分发仍由 MI-36～MI-39 承担。
