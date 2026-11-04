# 本地辅助输入后端与模型包（开发入口）

当前功能不接入日常输入，默认基础输入行为不变。这里的工具用于 MI-03/04/05/11
验证，不代表 Windows/Android 智能候选、设置入口或发布制品已交付。

## 构建与运行

Windows 使用 Rust 1.99.1、Visual Studio C++、与 VS 版本匹配的 CMake，以及 LLVM
libclang。`.cargo/config.toml` 同时设置 Rust、llama.cpp 与 CMake 的静态 MSVC CRT，
避免混合 CRT 导致链接失败。若曾以动态 CRT 编译 llama.cpp，需重新配置该构建目录。

```powershell
pwsh -NoProfile -File model_input/build.ps1 -Test -OnnxRuntime C:/runtimes/onnxruntime.dll
pwsh -NoProfile -File model_input/build.ps1 -Target android-arm64
pwsh -NoProfile -File model_input/build.ps1 -Target android-x64
```

ONNX Runtime 必须由应用构建/发布流程提供，不从模型包加载。Windows 本次开发验证
使用官方 `onnxruntime==1.31.1` wheel 内的原生 DLL（19,288,376 字节，SHA-256
`e45f61e50d8650885452be538a00bb14a715e46a1bc3691000ce947e89ed0306`），
Rust `ort=2.0.0-rc.15` 使用 API 31。wheel 只是开发机取得原生制品的途径，用户运行
不依赖 Python。分发时仍须携带 ONNX Runtime、第三方依赖的许可及对应运行库依赖。
构建脚本只复制显式传入的 `MYWUBI_ORT_DYLIB`，校验 PE/ELF 与目标 CPU 架构。
Android 交叉构建忽略继承的 Windows DLL，需显式传入对应 SO 才复制运行库；
错误架构会提前拒绝。脚本不会联网安装或从模型执行脚本。
Android 由应用在加载模型之前调用 `initialize_runtime(nativeLibraryDir/libonnxruntime.so)`；
实际 AAR/SO 集成、16 KB 页验证及真机运行属于 MI-14/18/19，目前没有验收。

llama.cpp 固定 `llama-cpp-2=0.1.161` / 同版 sys crate（MIT OR Apache-2.0），关闭
GPU、OpenMP 以及其它硬件后端，2 推理线程、1024 token 上下文。
Android 构建固定 NDK 27.2.12479018、API 26、静态 C++ 库，目标链接 16 KB 页对齐。
生成 Rust 库不能证明最终 APK 的 ABI/页大小或设备兼容性。

```powershell
$env:MYWUBI_TEST_GGUF = 'C:/models/qwen2.5-0.5b-instruct-q4_k_m.gguf'
cargo test -p model_input --features llama-cpp-2 --test llama_real -- --ignored
cargo run -p model_input --features llama-cpp-2 --bin score-llama -- $env:MYWUBI_TEST_GGUF model_input/evaluation/request.json
cargo run --release -p model_input --all-features --bin compare-models -- $env:MYWUBI_TEST_GGUF
```

## GGUF 续接评分

本次真实模型来自 [Qwen 官方模型库](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF)，
Apache-2.0，revision `9217f5db79a29953eb74d5343926648285ec7e67`，Q4_K_M 文件
491,400,032 字节，SHA-256
`74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db`。
开发机直连失败，实际经 hf-mirror 下载，文件哈希与该 revision 的 LFS oid 一致。
模型本体不进入 Git 或基础安装包。它只是首轮实验对象，没有被列为质量达标推荐。

`gguf-continuation-v1` 使用原始前文续接的 token 对数概率均值。前文与候选分别分词，
显式固定边界，不把待输入编码加入已提交前文，不启用聊天模板、思考或长文本生成。
不同模型/分词器的均值不能直接比较，长度偏差需独立校准。空前文用换行初始化；
超过 token 上限明确失败。适配器仅声称支持 llama.cpp 实际可加载且可清除候选分支的
架构，自定义决策头或仅改成 GGUF 后缀不构成支持证明。

相同前文复用 KV 与前文 logits，计算前文的 log-normalizer 一次，候选分支逐一移除。
任何错误或取消清空状态；更换编辑器/上下文时由契约守卫 reset。
取消在前文/候选 token 解码之间合作检查，单次 native decode 不保证立即中断。
模型/上下文随对象释放；进程级 llama 初始化元数据保留到进程结束，以免并发 init/free。
真实 CPU 测试验证候选倒序不改变身份分数，以及 reset 后重新计算一致。

## ONNX character-step-v1

固定支持嵌入权重、float32、opset 17 样例；算子实际兼容性由 ORT 加载和真实探测确定。
不接受外部权重文件定位，使用内存模型加载，最大模型 64 MiB、词表 1 MiB。
此适配类型不是任意 Hugging Face Transformer ONNX 图的通用加载器。

| 张量 | 类型 / 形状 | 语义 |
| --- | --- | --- |
| x | f32 [1,1,V] | 当前字符的 one-hot |
| h、c | f32 [1,1,H] | 调用方持有的分支状态 |
| logits | f32 [1,V] | 下一个字符的未归一化分数 |
| next_h、next_c | f32 [1,1,H] | 更新状态 |

H 为 1～64，V 为 3～10000；词表 JSON 指定 `adapter`、`hidden_size`、`tokens`，
0 为 `<unk>`、1 为 `<bos>`，其它项是唯一 Unicode scalar。状态通过 BOS 和实际前文
逐步更新，每个多字候选从同一前文状态独立分支，使用字符对数概率均值。未知字符映射
到 UNK 是明确限制，不能当作中文长尾已解决。重复前文可复用状态，编辑后重新构建。

样例生成（仅开发者导出需要 Python）：

```powershell
python -m pip install numpy==2.5.4 onnx==1.23.2 onnxruntime==1.31.1
python model_input/tools/export_samples.py model_input/evaluation/onnx
```

`counts.onnx` 从 MI-01 train 文本的字符转移计数导出 MatMul 评分图；`lstm.onnx`
是固定 seed 42 的随机权重接口样例。二者不能冒充现成且达标的输入法模型。
`numerical-reference.json` 保存独立 NumPy 前向计算，Rust ORT 测试逐项比较状态与
logits，误差上限 1e-6。坏形状、未知算子样例必须被加载探测拒绝。

## 数据包规范 v1

包是目录，`manifest.json` 具有以下必填元数据：

- `protocol: 1`、便携 `id` / `version`（字母、数字、短横线、下划线，不含 `--`）。
- `backend: LlamaCpp | OnnxRuntime`、固定版本的 `adapter`。
- `model`、ONNX 的 `vocabulary`，以及含相对路径/字节数/SHA-256 的 `files`。
- `source`、`license`、`capabilities`（同 MI-02 协议的任务/语言/限制/状态/取消声明）。

示例与请求模型由离线工具生成，当前包 API 支持校验、原子导入、列出、加载及移除
非活动版本。生成者必须自行拥有对应权重/词表的分发权利。

```powershell
python model_input/tools/package_model.py onnx model_input/evaluation/onnx/counts.onnx target/count-package --vocabulary model_input/evaluation/onnx/vocabulary.json --id authored-counts --version v1 --source authored-MI01-train --license MIT
cargo run -p model_input --bin model_packages -- validate target/count-package
cargo run -p model_input --bin model_packages -- import target/count-package target/model-store
cargo run -p model_input --all-features --bin model_packages -- score target/model-store/authored-counts--v1 model_input/evaluation/request.json
```

读取只允许固定数据扩展名；拒绝绝对路径、`..`、反斜杠、设备名、大小写重复名称、
符号链接/reparse point、未列出文件、脚本与 DLL。32 文件/4 GiB 是导入上限，
不代表这些大小的模型符合设备内存预算。Graph 输入输出仍须运行时二次验证。
导入采用新 staging 目录，复制后重新验证，再原子改名；同 ID/version 不覆盖。
失败不改变已有目录。替换应先导入新版本、成功加载，再更新当前选择并失效旧请求；
`ModelStore` 在工作线程持有当前选择，先验证并加载替代模型，成功后增加 generation，
旧版本请求返回 Stale。失败保持原会话可用，卸载后不再持有模型。加载时核对包声明的
能力与实际适配器完全一致，不能用伪造声明开启不支持的能力。替换峰值可能暂时包含
两个模型，服务需另行检查设备预算。选择持久化、升级保留及 UI/服务生命周期尚未接入。
包 API 不等同于完整的双端模型管理界面。

同一进程跨后端切换验证入口（需先导入对应 ID/version）：

```powershell
cargo run --release -p model_input --all-features --bin model_packages -- switch target/model-store model_input/evaluation/request.json authored-counts@v1 qwen25-05b@q4km-v1 authored-counts@v1
```

参考：[llama-cpp-2 API](https://docs.rs/llama-cpp-2/0.1.161/llama_cpp_2/)、
[ORT Session API](https://docs.rs/ort/2.0.0-rc.15/ort/session/struct.Session.html)。
