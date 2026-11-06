# MI-34 Windows ONNX 加速评估

2026-11-06 决策：暂不启用 ONNX GPU。保留现有 ORT CPU 和 `character-step-v1`
协议；不向设置界面提供 DirectML/WinML 选项。此结论是可复现的暂缓决定，不是
“ONNX GPU 没有收益”的测量结论，也不表示完成了 GPU 推理验收。

## 当前模型与接口证据

检查 `model_input/src/onnx.rs`、`tools/export_samples.py`、`catalog/models.json` 和
`evaluation/onnx/`：目前只有训练文本字符计数 MatMul 图和固定随机权重 LSTM 接口
样例，没有经过独立预测质量验收的产品 ONNX 模型。目录中的下载模型是 GGUF。
不能用这些样例的数值误差或运行速度证明产品预测质量和加速收益。

现有协议限定嵌入权重、float32、opset 17：输入 `x [1,1,V]`、`h/c [1,1,H]`，输出
`logits [1,V]`、`next_h/next_c [1,1,H]`，V≤10000、H≤64。逐字符在 CPU 建立张量、
运行后读回 logits/状态；候选逐支续算，`proposes_words=false`。GPU 不会给该接口
增加生成能力，也不能让任意 Transformer ONNX 文件满足它。当前 `ort` 固定
2.0.0-rc.15 / API 31，随应用加载 ORT 1.31.1，模型包不能指定 DLL 或自定义算子。

## 路径比较

| 路径 | 兼容性与维护条件 | 本次选择 |
| --- | --- | --- |
| ORT DirectML EP | DX12 设备，Windows 10 1903 起；必须禁用 memory pattern、使用顺序执行；同一 Session 不并发 Run。需匹配 API 31 的 ORT DML 发行包、DirectML 运行依赖及许可；逐算子核实分区，不能把 CPU 分区计为 GPU | 暂缓 |
| 新 Windows ML | 保留 ORT API，系统或应用携带运行组件，可动态取得 EP；硬件专用 EP 需要 Windows 11 24H2+，CPU/DirectML 覆盖更早的受支持系统。需独立核实 Rust 绑定、API 31、EP 取得与离线安装策略 | 有产品模型后优先重新评估 |
| 系统内置旧 WinML | WinRT 接口和随系统变化的运行时不能直接替换本项目固定版本的 ORT C API DLL；需另建适配、数值与部署矩阵 | 不接入 |

DirectML 当前处于维护阶段，新增开发转向 Windows ML。以上平台约束来自
[DirectML EP 文档](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html)、
[ORT Windows 入口](https://onnxruntime.ai/docs/get-started/with-windows.html)及
[Windows ML 概述](https://learn.microsoft.com/en-us/windows/ai/new-windows-ml/overview)，
于 2026-11-06 核对。具体发行包的 API/算子/驱动兼容性仍须实际加载验证。

现有逐字符小图可能由传输与调度成本主导，这是接口分析推断，未测量净收益。
本次不下载安装 EP，不采集随机图速度作为产品证据；仅维持已有 CPU 数值回归。

## 重新开启的门槛与复现

1. 提供真实、可分发、符合协议的模型包，固定权重/词表/准入策略 SHA-256，及独立
   留出集质量基线；如需生成式 ONNX，先定义新适配协议。
2. 在独立模型进程中明确选择 EP/适配器，记录每个算子的实际分配和 CPU 回退原因。
   不从模型目录加载原生库。保存/热切换与取消、失败、设备释放沿既有工作线程协议。
3. 对同模型运行原有 NumPy/CPU 数值参考和候选结果对照，再测冷加载、预热、首次
   展示、P50/P95/P99、超时/有效展示比例、内存与至少 30 分钟稳定性。不得只报 Run
   或 tokens/s。只要输出退化、没有净收益或不满足运行组件许可，就保留 CPU。

复现本次判断：检查上述四处文件与 `docs/model-input-backends.md` 的样例来源，
运行带 `ort` feature 的模型测试。对 ONNX 配置手动 Vulkan/CUDA 时，服务返回明确
“当前 ONNX 适配仅启用 CPU”原因，模型身份、能力与准入策略保持不变。GPU 数值、
产品质量和 GPU 端到端测量均记为“不适用：未实施”，不得记作通过。
