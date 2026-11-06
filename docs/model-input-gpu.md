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
Windows 按需组件协议和 CPU 分发流程已实现，见[组件说明](model-input-components.md)。GPU 可沿用同一清单/打包协议，但正式 GPU 资产与设备验收仍独立保留待办。

## 自动选择与默认策略

默认仍为 CPU，自动模式需用户主动选择。只有 Windows 的提前加载并保持模式才
运行短基准；按需加载保留 CPU，避免将双后端筛选成本放入第一次输入。自动模式
在加载时要求接通电源、未开启节能模式、无系统低内存信号，并满足设备内存准入；
状态未知也保留 CPU。这些是加载时检查，不承诺拔掉电源后立即迁移已加载的模型。

通过准入后，以当前模型、线程数和候选数，对六个固定公开前文样例顺序测量 CPU
与一个候选 GPU。各样例须返回有效非空候选，CPU/GPU 的完整候选数组须逐项一致，
GPU 总耗时和最慢样例均须改善超过 15%。失败、超时、输出差异或收益不足都保留
CPU，并在执行状态显示原因；筛选内部先释放 CPU 权重再加载 GPU。该短基准只用于当前
进程的保守筛选，不是质量认证或总体 P95，不缓存为永久硬件结论，也不读取宿主
正文。加载/筛选本身仍可能受驱动首次编译和系统负载影响。

手动 CPU/Vulkan/CUDA 选择不变。自动选中的 GPU 仍使用同一运行错误回退机制；
本次模型 CPU 样例出现空输出，自动模式实际保留 CPU。分位、展示率与输出差异
必须一起解释，不能只取 GPU 更快的成功请求宣称同等质量的净收益。

## 可复现的共享会话与独立服务测量

构建 `probe-pipeline` 后，在没有其他 model-worker 占用当前用户管道的测试会话中运行：

```powershell
./scripts/measure_model_pipeline.ps1 -BinaryDirectory target/mi-gpu/release `
    -Store target/mi28-cross-source-store -ModelId qwen25-words -ModelVersion q4km-v1 `
    -Device vulkan -Seconds 1800 -Iterations 60 -OutputPrefix target/vulkan-stability
python model_input/tools/report_device_comparison.py target/vulkan-stability `
    --output target/vulkan-stability-summary.json
```

脚本记录制品哈希、每次共享 Session 确认提交至独立服务 IPC 再到候选发布的耗时，
以及工作进程资源；1000 ms 等待预算不变，超时和无候选均进入展示比例分母。
该边界不含真实 TSF 宿主的编辑回调、GDI 绘制或屏幕扫描，不能称为屏幕可见延迟。
`on_demand` 可测首次请求触发加载，`preload_keep` 记录就绪等待，持续测量前另有
10 秒空闲驻留。分位采用 nearest-rank，仅统计首个请求之后的有效候选发布；
缺失的 GPU 计数器、功耗和温度保留 null。完整原始记录、限制和结论见
[MI-32～MI-35 验收记录](validation/mi32-35-review.md)。

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

## CUDA 组件开发入口（未完成 NVIDIA 真机验收）

`cuda` 是独立 feature，复用同一 GGUF 加载、设备名称、预测、回退和准入路径；
CPU/Vulkan 构建无需 CUDA Toolkit。构建时显式指定 `CUDA_PATH` 与 SM 架构，避免
发布制品只包含构建机 GPU 的代码。默认编译目标 75/80/86/89 是待验证范围，
不是兼容认证；其他 SM 需与所选 Toolkit 核对后显式提供。

```powershell
$env:CUDA_PATH = 'C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8'
$env:CARGO_TARGET_DIR = 'target/mi-cuda'
./model_input/build.ps1 -Acceleration cuda -CudaArchitectures '75;80;86;89' -Release
./target/mi-cuda/release/probe-device.exe PACKAGE cuda - 3
./scripts/stage_gpu_component.ps1 -Worker target/mi-cuda/release/model-worker.exe `
    -Backend cuda -Destination target/cuda-component -OnnxRuntime C:/runtimes/onnxruntime.dll `
    -CudaRuntimeDirectory "$env:CUDA_PATH/bin" -CudaLicense C:/runtimes/CUDA-EULA.txt `
    -CudaArchitectures '75;80;86;89'
```

固定绑定的 sys crate 在 Windows 动态链接 cudart、cublas、cublasLt 和 nvcuda。
制品脚本要求恰好一组对应 DLL、匹配版本的 NVIDIA 许可和 SM 清单；验证实际
启动、导出能力和哈希后才生成组件清单。nvcuda.dll 由 NVIDIA 驱动提供，不从 SDK
复制驱动，也不包含开发工具或模型权重。需要按实际导入表检查是否还有新增依赖。
CUDA 12.8 的版本/驱动匹配参见 [NVIDIA 发布说明](https://docs.nvidia.com/cuda/archive/12.8.0/cuda-toolkit-release-notes/index.html)；
该版本附带 Windows 571.96 驱动，不能将 12.x 的最低小版本兼容驱动数字直接当作
本组件的完整硬件认证。分发文件范围和条件须与实际 Toolkit 的
[CUDA EULA](https://docs.nvidia.com/cuda/archive/12.8.0/eula/index.html)对应。

本机没有 NVIDIA GPU/Toolkit。已验证请求 CUDA 时明确回退 CPU，缺 Toolkit
构建会提前失败；未构建或发布 CUDA 二进制，未宣称 CUDA 加速有效。必须补做
真实同模型冷/热预测、输出对照、设备失效/显存不足和释放，以及运行库许可与
制品验证后，才能勾选 MI-33 总任务。用户同意保留这部分实测待办。
