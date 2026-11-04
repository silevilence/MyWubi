# MI-03 llama.cpp CPU 评分验收

2026-11-04。实现与构建入口在 `model_input`，由 `llama-cpp-2` 可选 feature 启用，
TSF DLL 不依赖或加载推理库。固定 `llama-cpp-2`/sys 0.1.161，CPU、2 线程、
1024 token context，GPU layers=0、OpenMP 关闭。

真实模型：[Qwen/Qwen2.5-0.5B-Instruct-GGUF](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF)，
Apache-2.0；revision `9217f5db79a29953eb74d5343926648285ec7e67`；
Q4_K_M，491,400,032 字节，SHA-256
`74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db`。
直连超时，下载使用 hf-mirror，哈希与固定 revision 的 LFS oid 核对一致。
权重不进入 Git 或基础安装包。

评分直接读取候选续接 logits，计算每 token 对数概率均值，没有思考/解释/聊天输出。
前文与候选分别分词，边界和长度策略固定；不保证任意架构或自定义决策头兼容。
相同前文复用 KV，候选分支逐个清除；编辑器/上下文变更或错误调用 reset。
取消在 native decode 之间检查，不能承诺中断一次已开始的 decode。
模型与 context 在对象销毁时卸载，进程级后端元数据保留以避免 init/free 竞争。

验证记录：

- Windows 真实 CPU 测试通过：候选倒序后身份分数一致，reset 后重算一致。
- `score-llama` CLI 对“下周去”前文、“西安/仙”候选真实评分，输入文件不含目标答案。
- 缺文件返回 MissingModel，非法格式返回 UnsupportedModel，推理/期限/取消沿 MI-02 契约回退。
- ARM64 Rust 库成功交叉构建（NDK 27.2.12479018、API 26）；不等于 ARM64 真机运行。
- 本机 CPU 为 Core Ultra 7 165U。调试构建的初次 load 约 453 ms、首轮双候选约 128 ms；
  该记录仅证明调用闭环，不代替 MI-05/17 的效果和端到端性能门槛。

复现：

```powershell
$env:MYWUBI_TEST_GGUF = 'C:/models/qwen2.5-0.5b-instruct-q4_k_m.gguf'
cargo test -p model_input --features llama-cpp-2 --test llama_real -- --ignored
cargo run -p model_input --features llama-cpp-2 --bin score-llama -- $env:MYWUBI_TEST_GGUF model_input/evaluation/request.json
```

完整双后端构建说明和现有产品接入限制见 [后端说明](model-input-backends.md)。
