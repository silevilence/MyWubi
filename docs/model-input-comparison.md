# MI-05 首轮模型对比与接入决策

2026-11-04，结论：当前两份现成 GGUF 模型均不适合启用辅助排序，ONNX 统计样例也
没有首选净收益。暂不选定产品推荐模型，保持基础输入；不能因为后端可运行就启用。
MI-05 整体仍未完成：缺独立规模数据、设备资源/端到端指标以及合适的现成 ONNX 模型。

## 对象与可追溯性

| 对象 | 来源与许可 | 评分/部署条件 | 本轮定位 |
| --- | --- | --- | --- |
| Qwen2.5-0.5B-Instruct Q4_K_M | [Qwen 官方 GGUF](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF)，Apache-2.0 | 491,400,032 B，GGUF 内嵌分词器，CPU llama.cpp | 现成中文语言模型 |
| Qwen3-0.6B Q4_0 | [ggml-org 转换包](https://huggingface.co/ggml-org/Qwen3-0.6B-GGUF)，Apache-2.0；[原始模型](https://huggingface.co/Qwen/Qwen3-0.6B) | 428,970,080 B，GGUF 内嵌分词器，CPU llama.cpp | 换模型与不同量化的对照 |
| authored-counts | 仓库自编 train 文本，MIT | 8,489 B，字符词表，ONNX CPU MatMul 图 | 轻量统计对照，不是预训练产品模型 |
| LSTM 导出样例 | 自编固定随机权重，MIT | 固定图协议、CPU ONNX | 仅验证接口和状态，不参与质量选型 |
| SmolLM2-135M | [HuggingFaceTB 模型卡](https://huggingface.co/HuggingFaceTB/SmolLM2-135M)，Apache-2.0 | 模型卡明确主要面向英语 | 调研后未作为中文替代品实测 |

精确 revision、文件长度和 SHA-256 见 `model_input/evaluation/results/models.json`。
两份 GGUF 经 hf-mirror 下载，并与固定 revision 的 LFS oid 核对；权重不提交 Git。
运行库与线程配置见 [后端说明](model-input-backends.md)，统一 2 CPU 线程、1024 token。
中文能力只是模型任务范围，不构成当前输入法重排质量保证。没有发现并验证符合
character-step-v1 的现成中文专用权重，不能把统计图与随机 LSTM 充作这一缺口的答案。

## 同一候选和前文下的质量

数据为 38 个自编工程回归用例，train/dev/test=8/10/20。模型只接收输入位置之前的前文
及合法候选，不接收目标答案。以下 test 排序 12 例、联想 8 例；不是独立产品质量估计。
本轮没有改变冻结的候选召回，排序遗漏的 1 例与联想遗漏的 2 例不能靠重排修复。

| 排序方法 | 首选 / 12 | Top-3 / 12 | 原正确改错 / 原正确 5 例 | 原错误改对 | 翻页次数 |
| --- | --- | --- | --- | --- | --- |
| 现有码表基础排序 | 5 | 8 | 0 | 0 | 1 |
| Qwen2.5 | 5 | 10 | 2 | 2 | 4 |
| Qwen3 | 5 | 9 | 2 | 2 | 10 |
| ONNX 字符统计 | 4 | 9 | 2 | 1 | 1 |

歧义子集只有 3 例：基础/Qwen2.5/Qwen3/统计图首选分别为 1/1/0/1。
联想各方法都是召回与首选 6/8，没有产生新增候选或新的收益。
两份 GGUF 在原本正确的 5 例中各改错 2 例，与预设 ≤1% 的误改门槛相差明显；
不能把 Top-3 改善或固定首位的保护效果称作首选净收益。

## 单次开发机调用采样

Core Ultra 7 165U，12 核/14 逻辑处理器，33,905,598,464 B RAM，Windows x64，
Release 构建。构建完成后串行运行，未同时执行本任务的编译或另一个模型评测。
没有清除 OS 文件缓存，加载数字不是机器冷启动；每模型 27 个非空 dev/test 请求，
每个请求运行前文未缓存/已缓存两轮。以下是该短采样分位数，不是按键端到端验收。

| 模型 | 加载 ms | 前文未缓存 P50/P95/P99 ms | 已缓存 P50/P95/P99 ms | 截止前有效 / 27（未缓存/缓存） |
| --- | --- | --- | --- | --- |
| Qwen2.5 | 380.16 | 71.51 / 338.02 / 379.57 | 26.52 / 277.49 / 291.07 | 19 / 20 |
| Qwen3 | 585.84 | 72.25 / 264.00 / 275.03 | 20.16 / 214.60 / 215.05 | 19 / 22 |
| ONNX 统计（第一轮） | 58.84 | 0.152 / 1.120 / 1.405 | 0.072 / 1.043 / 1.173 | 27 / 27 |

排序截止固定 100ms，联想 250ms。其余 3 个空召回请求返回 Empty，未混入成功时延。
raw JSON 同时保存包含迟到结果的离线质量和截止回退质量：排序回退基础候选，联想
超时不显示。大候选集合的逐候选多 token 评分仍然昂贵，缓存未解决截止有效率问题。

尚未执行 1000 次/组合、30 分钟持续输入、RSS/PSS/CPU/温度、宿主按键→显示→上屏
或 ARM64 真机测量；不能将此表作为 MI-17/18 验收。也不能从一个 CPU 推广到普通手机。

## 复现与决策

```powershell
cargo run --release -p model_input --all-features --bin compare-models -- C:/models/qwen2.5-0.5b-instruct-q4_k_m.gguf
cargo run --release -p model_input --all-features --bin compare-models -- C:/models/Qwen3-0.6B-Q4_0.gguf
```

完整原始分数、失败原因、每次时延和分组指标分别保存在
`model_input/evaluation/results/qwen25-cpu.json` 与 `qwen3-cpu.json`。
这些文件只含自编样例标识和数值，不含用户真实输入。

当前决策为“更换/优化模型并暂不启用”，不是“现成模型均无用”或“必须训练”。
先补充有合法来源的独立数据与召回，再检验适合该任务的现成评分模型。
微调/蒸馏是否值得投入的条件评估见 [MI-20](model-input-adaptation.md)。
