# Windows 可选模型加速

模型仍在 `model-worker.exe` 中执行，TSF DLL 不链接 llama.cpp 或 GPU 库。CPU 独立
构建继续可用，默认配置为 `device="cpu"`；GPU 不提高模型准确性，也不改变模型
身份、生成约束、置信度门槛或候选发布期限。

## 设备与失败行为

`[prediction]` 的 `device` 支持 `cpu`、`auto`、`vulkan`、`cuda`；`device_name`
是后端发现的名称（如 `Vulkan0`），空字符串选择该后端第一个匹配设备。设备序号
不会作为跨机器身份保存。Windows 设置读取所安装工作进程的能力和设备列表，
未包含的加速选项禁用；保存配置后通过原有重载流程重新加载。Android 显示保存的
请求，但只执行 CPU，可明确将选择重置为 CPU。

独立进程支持 `--devices` 和 `--features`；IPC `Devices` 返回设备名称、说明、
后端、总量和可用内存，`Execution` 返回请求、实际设备和回退原因。`Status` 的
旧相位格式保持兼容。只有 Ready 才对外展示执行设备；暂停、卸载、失败和准备中
不伪报已生效。发现硬件不等于模型兼容或完成质量验收。

GGUF 手动 GPU 加载固定一个设备、请求卸载全部模型层；缺组件/设备或保守显存
检查失败回退 CPU。原生加载/上下文创建失败也重试 CPU。运行时返回可恢复的
Inference/Load 错误时先释放 GPU，再加载 CPU，只重试当前尚未发布的计算；相同
实例之后保持 CPU。超时、取消、低置信度和非法候选不触发重算。所有返回仍受原
请求 deadline 和版本检查约束。显存检查为模型文件合计加 256 MiB，不能代替
原生分配结果；不保证任意架构/驱动可用。

驱动令进程崩溃或挂死属于原生运行限制，不能包装成 Rust Result 回退成功；基础
输入与模型进程隔离。GPU 可执行文件依赖驱动提供的 `vulkan-1.dll`，如该程序无法
启动，应保留并使用独立 CPU 组件。不要把 GPU 工作进程覆盖为所有用户的唯一制品。
组件自动下载、安装和版本切换由 MI-36～MI-39 跟进。

## Vulkan 构建与可选制品

固定 `llama-cpp-2`/sys 0.1.161。开发机使用 VS C++、LLVM libclang 和 Vulkan SDK
1.4.363.1；终端用户只需要匹配的 GPU 驱动，不需要 SDK 或编译器。SDK 官方
[copy_only 模式](https://vulkan.lunarg.com/doc/view/1.4.309.0/windows/getting_started.html)
可将开发文件复制到指定目录而不修改系统注册表/路径。采用 Ninja，避免当前
MSBuild + 上游 TrackFileAccess=false 导致着色器 ExternalProject 步骤乱序。

```powershell
$env:VULKAN_SDK = 'C:/SDKs/Vulkan/1.4.363.1'
$env:CARGO_TARGET_DIR = 'target/mi-gpu' # 不与旧 MSBuild CMake 缓存混用
$env:CARGO_BUILD_JOBS = '4'
./model_input/build.ps1 -Acceleration vulkan -Release
./target/mi-gpu/release/model-worker.exe --devices
./target/mi-gpu/release/probe-device.exe PACKAGE vulkan - 3
./target/mi-gpu/release/probe-device.exe PACKAGE cpu - 3
./target/mi-gpu/release/probe-device.exe PACKAGE vulkan MissingDevice 3
./scripts/stage_gpu_component.ps1 -Worker target/mi-gpu/release/model-worker.exe `
    -Backend vulkan -Destination target/vulkan-component -OnnxRuntime C:/runtimes/onnxruntime.dll
```

制品脚本要求新目录，校验 PE x64、实际启动能力，带上 ORT、VC 运行依赖和模型
运行库许可证，生成每个文件的 SHA-256/大小清单。Vulkan SDK/驱动/模型权重不进入
组件。Vulkan-Headers 的版权和 MIT 许可及 llama.cpp MIT 许可随制品提供。
首次驱动着色器编译可能显著增加加载时间，必须与后续进程冷加载分别记录。

原生 probe 记录包清单、load、warmup、每次预测结果与执行设备、关闭耗时；这是
适配器测量，不能替代 TSF/JNI 首次可见预测。跨厂商未测范围、实际测量与端到端
结论见本系列验收记录。ONNX 决策单独见 [MI-34 评估](model-input-onnx-gpu-decision.md)。
