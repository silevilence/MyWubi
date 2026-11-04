# MI-04 ONNX 适配进展

2026-11-04：`model_input` 提供 `ort` 可选 feature，固定 ORT Rust 2.0.0-rc.15、
原生 ONNX Runtime 1.31.1、CPU execution provider。通过应用部署的动态库加载，
不从模型包选择或执行原生代码。Windows 测试与 Android ARM64 库交叉构建通过。
Android 最终 SO/APK 与真机运行仍属于 MI-14/18，尚未验证。

`character-step-v1` 接受 float32 的 x[1,1,V]、h/c[1,1,H]，输出 logits[1,V]、
next_h/next_c[1,1,H]。V=3～10000、H=1～64，不支持动态批次或任意 Transformer 图。
词表前两项为 UNK/BOS，其余为唯一 Unicode 字符。模型最大 64 MiB，嵌入权重；
调用 ORT 的内存模型加载接口。中文多字候选从相同前文状态独立分支，使用平均字符
对数概率评分；未知字映射 UNK。状态不会跨候选或编辑器复用。

导出工具 `model_input/tools/export_samples.py` 使用独立 NumPy 前向实现验证：
字符统计图误差为 0，LSTM 最大绝对误差约 2.98e-8，验收上限 1e-6。
Rust 测试验证相同数值、重复评分、分支倒序、重置、坏形状与未知算子拒绝。
导出的 ONNX 和数值参考已随仓库保存，运行 Rust 评分不需要 Python。

统计图只学习 MI-01 自编 train 文本；LSTM 使用固定随机权重，只用于接口验证。
尚无符合该图约定、来源许可已核实的现成中文专用模型，因此 MI-04 保留整体未完成，
不以样例图或 Qwen 的 GGUF 运行结果代替该缺口。

复现：

```powershell
pwsh -NoProfile -File model_input/build.ps1 -Test -OnnxRuntime C:/runtimes/onnxruntime.dll
pwsh -NoProfile -File model_input/build.ps1 -Target android-arm64
python -m pip install numpy==2.5.4 onnx==1.23.2 onnxruntime==1.31.1
python model_input/tools/export_samples.py model_input/evaluation/onnx
```

Python 只用于开发者重新导出。ONNX Runtime 原生库及第三方许可必须由发布流程携带；
目前没有改动用户基础安装包，也没有宣称任意 `.onnx` 文件可直接使用。
