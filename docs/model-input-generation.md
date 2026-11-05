# MI-21～MI-26 生成式字词预测

## MI-21 契约（2026-11-05）

评分接口保持 v1 和严格的候选身份覆盖校验。生成使用独立 `ProposalRequest` /
`ProposalResponse`，声明 `proposes_words` 才可调用；只接受已确认光标前文。
没有词典召回时，选定模型的会话仍发出预测工作，评分后端返回 Empty。
生成输出无需出现在任何输入法词典中，不影响编码候选的合法性。

适配器负责单个词或当前词剩余片段的语言边界；核心只做结构校验：1～16 个
完整汉字、最多 64 项、禁止重复与重复前文、禁止解释/标点/替换字符。
字符计数不代表 token 数或词边界。数组位置绑定当前 query 作为候选身份，
响应必须匹配全部版本；期限/取消由单调时钟控制，迟到结果不可发布。
静默停止用 Empty / LowConfidence，缺策略用 Uncalibrated，旧评分包不改变能力。

验证：`cargo test -p core_engine --test proposal_contract --test assistance_contract
--test prediction_session --locked`。覆盖词典外多字结果、空召回、重复、非法片段、
期限、取消、过期版本和评分后端兼容；生成后端与双端宿主验收记录见后续段落。

## MI-22 真实 GGUF 生成（2026-11-05）

使用原有固定 revision 的 [Qwen2.5-0.5B-Instruct-GGUF](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF)，
Q4_K_M、Apache-2.0，权重 SHA-256 及生成参数在
`model_input/evaluation/generation-policy.json`。GGUF 哈希也绑定内嵌分词器。
新增适配器 `qwen25-short-words-v1` 使用固定 ChatML 提示、greedy 解码和有界 JSON
语法；最多 64 token、3 候选、每项 6 字。解码拼接字节，完整 JSON/UTF-8 才发布。
空前文和句末停止，重复词/前文重叠撤下，解释、未完成字符和超长生成不会成为词条。
准入阈值只属于此生成策略，不复用评分阈值，不是准确率。

本轮开发提示经过调整（允许 JSON 空白并增加独立示例），参数未接触 MI-26 保留集。
真实测试验证“明天去”产生多个词典召回以外候选，空前文/句末停止，以及 decode
期间取消、超时返回错误；旧评分真实测试继续通过。15 项模型模块测试和 3 项
生成契约测试通过。Release 自编探针的全部结果见
[原始记录](validation/mi-generation-probe.json)。其中“研究量子”的“化学/生物学”
并不合理，不能将功能验收误读为质量达标。

复现：

```powershell
$env:MYWUBI_TEST_GGUF = 'C:/models/qwen2.5-0.5b-instruct-q4_k_m.gguf'
cargo test -p model_input --features llama-cpp-2 --test llama_real -- --include-ignored
cargo run --release -p model_input --features llama-cpp-2 --bin generate-words -- $env:MYWUBI_TEST_GGUF model_input/evaluation/generation-policy.json model_input/evaluation/generation-prefixes.json
python model_input/tools/package_model.py llama $env:MYWUBI_TEST_GGUF C:/model-package --generation-policy model_input/evaluation/generation-policy.json --id qwen25-words --version v1 --source https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF --license Apache-2.0
```

包清单新增可选 `generation_policy`，缺省保持评分旧包行为；文件哈希、模型哈希、
适配器、能力和预算均在导入/激活时验证。不在输入日志记录前文或生成内容。

## MI-23 双端接入（实现完成，宿主验收中）

IPC/JNI 使用兼容旧评分 JSON 的响应联合类型。生成结果经版本和边界校验后，
进入共享预测视图；编码候选不改变。新编码/失焦/外部编辑清除旧结果；只有宿主
确认成功才追加前文，失败保留可重试候选，未知写入不重放。
两端模型选择接受独立生成策略；候选中没有“续接/加载中”等状态文本。

主机验证：17 项共享预测会话测试通过，含 LegacyV1 与 TouchV1 的词典外候选
提交、拒绝重试、过期点击、等待期间空格放行、新编码优先及前文确认。
Windows Release DLL/服务与 Android 双 ABI 模型版已构建；实际宿主结果另记，
不能仅以构建和模拟会话测试勾选 MI-23。
