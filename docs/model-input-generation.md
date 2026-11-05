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

## MI-23 初次接入与失败记录

IPC/JNI 使用兼容旧评分 JSON 的响应联合类型。生成结果经版本和边界校验后，
进入共享预测视图；编码候选不改变。新编码/失焦/外部编辑清除旧结果；只有宿主
确认成功才追加前文，失败保留可重试候选，未知写入不重放。
两端模型选择接受独立生成策略；候选中没有“续接/加载中”等状态文本。

主机验证：17 项共享预测会话测试通过，含 LegacyV1 与 TouchV1 的词典外候选
提交、拒绝重试、过期点击、等待期间空格放行、新编码优先及前文确认。
Windows Release DLL/服务与 Android 双 ABI 模型版已构建；实际宿主结果另记，
不能仅以构建和模拟会话测试勾选 MI-23。

预加载后的 Android 设置搜索框尝试仍未显示生成候选，见
[多候选失败](validation/mi-generation-android-host-preloaded.log) 与
[单候选失败](validation/mi-generation-android-single-host.log)。Windows Sandbox
也实际加载新版 DLL，编码 `v` 显示“北京”并经空格确认后，没有观察到“市”的
生成候选；未完成生成选词及连续选择验收。保持 1000 ms 产品预算，不接受迟到
结果，也不以工作器 Ready 冒充宿主候选已显示。

关闭 Sandbox、结束全部构建后再次执行 Android 单候选测试，仍失败，工作器
状态为 Ready，见 [无并发构建重测](validation/mi-generation-android-quiet-host.log)。
这不能只归因于冷加载；现有状态也不足以区分推理超时、静默拒绝与显示链路原因，
需要后续针对失败请求做进一步诊断，不能把 Ready 当成结果成功。

整体审核补强：真实测试先检查无词典召回，再检查“北京 → 市”不在测试形码表、
全部内置预测词条及关联表中（用户词典为空）；这两种“词典外”不能混称。
解码语法按请求数量与策略上限的较小值生成，单候选请求不再多算三个候选。
[4 项真实后端测试](validation/mi-generation-real-tests.log) 全通过，包括该
完整词典外检查和取消/超时；离线功能验证不替代以上宿主失败。

加载策略与生命周期见 [MI-24 / MI-25](model-input-preload.md)，独立质量与适用
结论见 [MI-26](model-input-generation-evaluation.md)。当前模型仍不推荐启用。


## MI-23 修复与补充验收（2026-11-05）

先前 Ready 仍无候选的主要原因是实际生成没有在宿主的 1000 ms 期限内完成。修复缓存固定公共提示的 KV，预加载时以固定“预热完成”走完整生成路径；用户前文、生成 token 与评分分支缓存仍在请求/所有者变化时清除。语法 greedy 先检查全局最高分 token，再检查最高分的 32/256/2048 项，全部被拒绝才回退完整词表；分数、语法、提示、权重与 -3.0 门槛不变，同分保持原词表顺序。

尝试过批量推测 JSON 格式前缀，但真实等价性测试发现“明天去”的三候选输出发生变化，已完全移除；[失败证据](validation/mi23-rejected-framing-equivalence.log) 保留。压缩 JSON 新适配器和 Q4_0 权重实验也仅在自编开发提示上试验后放弃，没有发布新策略或使用保留集调参。

Windows Sandbox 的真实记事本已观察到“北京→市”的空格、数字 1、鼠标点击上屏；“天气→晴朗→多云”两步连续追加；新编码撤下预测后回车提交编码、切换英文后旧候选撤下、句末停止。外部编辑与配置重载后的旧选择不能追加。无候选时空格沿用普通行为；数字在中文空闲态仍按已有编码规则处理，不能把橙色预编辑数字误称为立即上屏。证据：[空格](validation/mi23-windows-generated-space.png)、[点击](validation/mi23-windows-generated-click.png)、[连续](validation/mi23-windows-generated-chain.png)、[切换英文](validation/mi23-windows-mode-withdrawal.png)、[新编码与回车](validation/mi23-windows-new-code-enter.png)。TIP 仅在隔离 Sandbox 注册，没有替换开发机输入法。

Android API 34 x86_64、4096 MiB、6 vCPU（模拟器上限），模型 4 线程，1000 ms、1 候选。最终无格式推测构建的三次原生生成耗时 654/608/605 ms，均返回“市”，见[设备记录](validation/mi23-android-final-deadline.json)。独立包 `org.mywubi.acceptancehost` 是真实系统 EditText，使用普通 InputConnection，没有模型或 IME 替身；[模块与复现](../android/acceptance-host/README.md)仅按需构建，不随输入法发布。

该宿主完成词典外词触摸上屏、编码优先撤下预测、外部编辑后重新预测、连续两次选择、句末停止及普通空格回退，[生成测试通过](validation/mi23-android-editor-generation.log)。[连续结果截图](validation/mi23-android-generated-chain.png)中的“天气晴朗多云”有语义矛盾，保留为质量失败，不能当成准确性证据。

系统设置搜索框仍可能因宿主负载超过 1000 ms：直接生成能在约 600 ms 完成，搜索宿主实测“天气→晴朗”接近 976 ms，下一步超时。[搜索宿主失败日志](validation/mi23-android-settings-budget-limit.log)保留。到期后撤下候选是产品的预算回退，不扩大期限，也不保证每个宿主、每次请求都显示生成结果；轻量真实宿主通过仅证明其验收范围内的显示与提交闭环。早期 4 vCPU / 格式推测构建的测量保留为历史，不能与最终 6 vCPU 数据直接合并比较。

Android 扩展回归最终 6 项通过、无跳过：词典预测、评分预测、低置信静默、隐藏/回收恢复、真实生成宿主和真实生成期限。旧可见性断言读到过期无障碍节点，已统一使用清缓存的既有断言助手，正文和候选要求不变。[逐项最终结果及失败/复测索引](validation/mi23-android-regression-summary.json)保留全部轮次。

最终工作器重新复制到全新 Sandbox 后，真实记事本再次通过空格确认“北京→市”和空格/数字 1 连续确认“天气→晴朗→多云”。[最终上屏](validation/mi23-windows-final-space.png)、[最终连续选择](validation/mi23-windows-final-chain.png)、[实际产物 SHA-256](validation/mi23-windows-final-artifacts.json)。

Sandbox 哈希记录对应截图时实际运行的产物。此后 MI-24 仅修正 `--config` 启动路径的写法，最终启动分支另由[首宿主复用测试](validation/mi24-host-path-after-fix.log)和[工作器哈希](validation/mi24-final-tested-worker.json)覆盖，生成解码路径未再改变。
