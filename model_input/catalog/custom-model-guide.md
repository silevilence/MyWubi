# 自定义模型接入与试运行

普通使用：展开「预测模型与资源」，选择下载源并下载已适配模型。程序自动补齐清单、许可和与权重绑定的策略；无需 Python、手写清单或阈值。下载后不会自动选择或开启预测。目前目录中的模型仅实验兼容，不推荐其预测质量。

## 支持范围

GGUF 评分适配 gguf-continuation-v1：已验证 qwen2（Qwen2.5）和 qwen3 架构，使用 GGUF 内嵌分词器，对已有候选独立分支评分，按 token 平均对数概率归一化。评分本身不会生成词典之外的字词。其他架构须先增加并验收适配，不能只改文件名。

GGUF 生成适配 qwen25-short-words-v1：仅 qwen2 / Qwen2.5 Instruct 的已验证聊天模板，生成完整短字词 JSON 数组。每次从明确的测试前文开始，不继承上次试运行状态。当前已验证权重为 Qwen2.5-0.5B-Instruct Q4_K_M，SHA-256 为 74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db。架构名称匹配仍不保证其他尺寸或量化可用。

ONNX character-step-v1：仅 CPU 字符递归图；float32 输入 x[1,1,V]、h/c[1,1,H]，输出 logits[1,V]、next_h/next_c[1,1,H]。V 为 3～10000，H 为 1～64，模型不超过 64 MiB，权重必须内嵌，不支持任意 Transformer 图、自定义算子或包内动态库。vocabulary.json 包含 adapter、tokens、hidden_size；tokens 前两项为 <unk>、<bos>，其余为唯一 Unicode 字符。初始状态全零，先输入 BOS；每个候选从同一前文状态独立分支，平均字符对数概率评分。模型和词表必须匹配。

## 开发者从本地文件准备

完整模型包目录必须包含 manifest.json 及其列出的全部数据文件。协议版本为 1；id/version 使用 1～64 位 ASCII 字母、数字、短横线或下划线，不能含双短横线。清单绑定 backend、adapter、capabilities、模型/分词器/策略路径、来源和许可，以及逐文件 bytes 和 sha256。禁止路径越界、符号链接、额外文件和可执行代码。总包不超过 4 GiB。

在项目根目录运行以下命令，把自己已取得的上述固定 GGUF 文件包装成实验包（只有开发准备步骤需要 Python）：

python model_input/tools/package_model.py llama C:/Models/qwen2.5-0.5b-instruct-q4_k_m.gguf C:/Models/my-qwen-package --generation-policy model_input/evaluation/generation-policy.json --id local-qwen --version v1 --source https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF --license Apache-2.0

该策略仅适用于上述精确权重。请保留上游许可证和来源；若增加许可文件，必须将其大小和哈希加入清单，不得随意追加未列文件。ONNX 使用同一工具的 onnx 模式，加 --vocabulary 和 --prediction-policy；详见仓库 docs/model-input-onnx.md 与 docs/model-input-contract.md。不要把示例统计图当成质量合格模型。

评分策略字段：version=1、adapter、model_sha256、vocabulary_sha256（GGUF 为 null）、minimum_score、minimum_margin、evidence。阈值为有限原始分数，margin 不小于零；必须记录独立开发集的校准过程，不能借用不同模型的阈值。

生成策略字段：version=1、adapter=qwen25-short-words-v1、model_sha256、max_tokens（8～96）、max_chars（1～8）、max_words（1～5）、minimum_mean_logprob（有限且不大于零）、evidence。生成参数与评分阈值含义不同，均不是正确概率。缺策略不能用于字词预测。

## 导入、校验与试运行

1. Windows 选择「导入模型包目录」；Android 在系统文件选择器选择完整包目录。校验失败保持当前模型和输入配置不变。成功后仅加入列表。
2. 打开「自定义模型指南与试运行」，选择已导入包，明确输入测试前文（内置样例为「北京」）。点击「试运行」才开始；不读取正在编辑的正文。
3. 查看候选、加载/预热耗时、推理耗时和失败原因。评分模型使用内置样例词典召回，生成模型直接生成；空候选可表示没有召回或未过准入。测试限制前文 256 字、CPU 1～4 线程、推理 50～1000 ms。加载与预热单独计时，不冒充实时延迟。
4. 通过只证明该样例可以运行，不能证明质量合格。要启用，仍须另行选择模型、主动开启总开关并保存。失败不会污染活动输入状态。

坏包：重新取得完整文件并核对哈希；不支持架构：使用已适配模型或实现新适配；缺分词器：补齐精确词表；缺预测策略：完成校准并提供与权重绑定的策略；缺运行组件：安装含对应后端的模型版发行包。不要通过关闭完整性检查解决错误。
