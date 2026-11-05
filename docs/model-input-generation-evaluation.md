# MI-26 生成式预测对照（2026-11-05）

当前 Qwen2.5-0.5B Q4_K_M + `qwen25-short-words-v1` 仅适合实验，不推荐启用。
新增的冻结保留集生成参考命中为 1/32，展示覆盖 9/41；词典与评分首选各命中 2/32。不能声称准确性提升。初次 33 项结果保留为历史，不能与新的 41 项合并成一次测试。

## 初次 33 项的数据与冻结

开发集是 MI-22 已使用的 7 个自编前文（MIT），包括搭配、领域词、词尾、空前文
和句末。保留集来自 [UD Chinese GSDSimp 固定版本](https://github.com/UniversalDependencies/UD_Chinese-GSDSimp/tree/4231dfd59866fa5999ad4a6bc1fdecd7985b3b59)，
属于 wiki 语料，贡献者 Peng Qi、Koichi Yasuoka；使用 CC BY-SA 4.0。
提取的文本及标注衍生物继续采用该许可，附
[许可全文](../model_input/evaluation/generation-corpus-LICENSE.txt)。底层 wiki 文本权利
仍属各原作者，不将标注许可误写成作者放弃文本权利。

全体外部来源留给保留集，开发材料不含该来源。全文 SHA-256 去重排序，499 个
合格句中固定取前 24 个，另取其中 8 个句末负例及空前文，共 33 项。排除 1 个
无有界实词位置的句子；其余 475 个未抽中，不是因结果失败而移除。分词标签用于
构建参考词，不传给模型。每第三项多字词只给首字，测试剩余片段。数据及完整
来源/散列/排除计数见 [保留集](../model_input/evaluation/generation-holdout.json)。

生成提示、greedy、3 候选/6 字/64 token、均值 logprob -3.0 在读取保留集之前
冻结，绑定模型及内嵌分词器哈希。评分阈值仅由开发集枚举选择，目标与网格写入
报告；后续不按保留集调参。词典/评分/生成获得相同前文；评分只能看到同一词典
召回。推理程序的输入类型只反序列化 id 与 prefix，忽略参考答案字段。
评分与生成共用同一模型，分支间 reset，模型看不到参考词或未来正文。
来源独立不代表这些公开文本从未进入模型预训练。

## 初次 33 项的全部结果

[机器报告](validation/mi-generation-report.json) 保留全部失败 id、输出、参考词，
另附 [开发原始输出](validation/mi-generation-dev-runs.json) 和
[保留集原始输出](validation/mi-generation-holdout-runs.json)。正例 24、停止例 9。

| 指标（离线 30 秒上限） | 词典 | 候选评分 | 直接生成 |
| --- | ---: | ---: | ---: |
| 参考召回 | 5/24 | 5/24 | 0/24 |
| 首选 | 0/24 | 1/24 | 0/24 |
| Top-3 | 4/24 | 1/24 | 0/24 |
| 展示覆盖 | 14/33 | 5/33 | 4/33 |
| 停止例误展示 | 0/9 | 0/9 | 0/9 |
| 词典字符串外参考命中 | 3/15 | 0/15 | 0/15 |

词典外统计检查内置词库、关联表及测试形码表全部词条（用户词典为空），附各
文件哈希。剩余片段按完整字符串比较，因此词典也可能产生“词条外的片段”；
不能把它误称为词典具备语言生成能力。另列“本次召回之外”19 项，三方法命中
均为零；这与全词典外是不同指标。

按产品 1000 ms 截止计算，生成覆盖为 1/33，7 次推理耗时超过预算（包括最终
静默返回的请求），晚到内容不计展示。词典/评分结果与上表相同。这里离线记录
用于截止模拟，不能代替真实宿主输入路径的延迟统计。

## 多种合理续接与连续选择的判定方法

精确匹配只衡量参考覆盖，不等于语义准确率。人工评审应隐藏方法/分数，将同一
前文的去重候选随机排序，由两名中文评审独立标注：可直接追加且自然、可接受但
需更多语境、不合理、不能判定。额外标记重复前文、破坏词尾、跨多个词、解释性
文字、应停止仍显示；分歧交第三人裁决，并报告一致率和未判定率。
连续选择每一步只追加当步候选，再用新的已确认前文请求，最多三步；逐步评价
是否仍自然，不能用原句后文偷偷引导选择。空结果记停止，不能剔除失败链。

本轮没有独立人工标注。以下是作者复核而非盲评结论：保留集中显示的四项为
`影片也从田纳西·威 → 田纳西`（重复较早前文、不能补完姓氏）、
`他宣布自己为 → 记者`（存在合理替代可能）、`然而再回到伦敦的暗 → 伦敦`
（破坏词尾/重复）、`在活动期间， → 参加`（需更多语境）。不能把 0 次参考命中
写成四项全部语义错误，也不能把两个可能合理的项算作已确认正确。
固定前八个样本的首选链只有第一项产生输出，追加“田纳西”后停止，首步已不合理；
其余停止未形成多步链。没有足够成功链支持连续语义质量结论。

初次对照未完整覆盖编辑前文及所有方法的连续选择，以下用新的保留集补齐。模型仍不作为质量达标的推荐项；后续调参必须使用新版本及另一份未触碰的保留集。

## 复现

```powershell
python model_input/tools/prepare_generation_evaluation.py
cargo build --release -p model_input --features llama-cpp-2 --bin evaluate-generation
target/release/evaluate-generation.exe MODEL.gguf model_input/evaluation/generation-policy.json model_input/evaluation/generation-dev.json > dev-runs.json
target/release/evaluate-generation.exe MODEL.gguf model_input/evaluation/generation-policy.json model_input/evaluation/generation-holdout.json > holdout-runs.json
python model_input/tools/report_generation_evaluation.py model_input/evaluation/generation-dev.json dev-runs.json model_input/evaluation/generation-holdout.json holdout-runs.json report.json
python -m unittest discover -s model_input/tools -p test_generation_evaluation.py -v
```


## 新的冻结验收：41 项、123 条配对链

[冻结方案](../model_input/evaluation/generation-acceptance-plan.json)在此次保留集推理前写定模型/策略/数据 SHA-256、评分网格与门槛、1000 ms 预算、首选追加规则和三步上限。权重仍为原 Q4_K_M、生成策略仍为 v1；没有放宽置信门槛。固定源码和许可证仍来自上文同一 UD 版本，按全文 SHA-256 排序跳过初次 24 个来源句，取随后 32 个正例、其中 8 个完整句末负例以及空前文，共 41 项。[数据](../model_input/evaluation/generation-acceptance.json)记录逐句来源、许可和选择偏移。499 个合格句中本次未选 467 个，另 1 个无有界实词位置的句子在推理前排除；没有根据模型结果排除失败。

每第四个正例包含一次外部前文替换，共 8 项。真实模型先读取另一个许可前文，再读取替换后的前文，随后 reset 并重算新前文，结果或停止原因必须一致。8/8 通过；这证明该回调边界无残留状态，不代表替换后的预测语义一定正确。开发集与保留集非空前文没有交集；公开 wiki 语料不保证未用于模型预训练。

词典、评分、生成在相同前文下评测。评分与生成使用相同权重的独立推理实例，避免离线交替评分破坏生成的公共预热缓存；不是产品每个宿主各加载一份权重。只有 prefix 被送入生成模型，参考答案由报告脚本读取，推理输入结构不反序列化参考字段。所有 41 个种子都执行三方法的最多三步首选链，每一步只追加本步实际可用且未超时的候选，空输出、失败和零步链均计入。

[开发原始输出](validation/mi26-final-dev-runs.json)、[保留集全部原始输出](validation/mi26-final-holdout-runs.json)、[机器报告及全部失败样本](validation/mi26-final-report.json)。评分门槛仍是开发集选定的 -6.0 / 1.0，报告程序验证开发复算与冻结方案一致。

| 指标 | 词典 | 候选评分 | 直接生成 |
| --- | ---: | ---: | ---: |
| 参考召回 | 5/32 | 5/32 | 1/32 |
| 首选 / Top-3 | 2/32 / 2/32 | 2/32 / 2/32 | 1/32 / 1/32 |
| 展示覆盖 | 23/41 | 8/41 | 9/41 |
| 停止例误展示 | 0/9 | 0/9 | 0/9 |
| 本次词典召回外参考命中 | 0/27 | 0/27 | 1/27 |
| 清单内词典字符串外参考命中 | 2/21 | 2/21 | 1/21 |

评分“参考召回”是进入评分的原始候选池召回；Top-3 才经过门槛。词典字符串范围是报告中带哈希的内置预测词库、关联表和 `tables/wubi86.dict`，用户词典为空；剩余片段也按字符串比较，不能把词典切出剩余片段称作生成能力。该轮初始生成全部在 1000 ms 内结束，最长约 513 ms，因此预算内与离线参考指标相同；停止例仅 9 个，不足以承诺普遍没有误展示。CPU 推理数据不代替 Android 高负载宿主延迟。

| 连续选择（每种方法 41 个种子） | 词典 | 候选评分 | 直接生成 |
| --- | ---: | ---: | ---: |
| 实际追加总步数 | 33 | 9 | 11 |
| 零步停止链 | 18 | 33 | 32 |
| 完成三步的链 | 2 | 0 | 1 |

全部 123 条链、所有 9 个非空生成样本及其 14 个候选，均在[代理定性分析](validation/mi26-assistant-semantic-notes.json)中保留。它明确标注 `reviewer_kind=assistant`、独立人工票数为 0，不作为盲评准确率。唯一三步生成链“它在位于亨茨维尔的→亨茨维尔→工厂→亨茨维尔”从第一步开始重复，第三步继续回环；“安德里亚·皮→安德里亚”破坏人名，“其同年→查尔斯/克拉克”重复前文。与此同时，“1897→年”自然，“而→湖”可能是合理的新分句，不能仅因不等于唯一参考就判错。词典也有“湾仔→细→看”“提及了→解→一下”等跨词边界问题；评分能筛去部分候选，但仍有“中学是一→本”之类语义类别错误。

[人工判定规程](validation/mi26-human-review-protocol.md)定义两人独立标注、第三人裁决、多解允许、未判定项与一致率。已生成 [246 条空白盲审条目](validation/mi26-blind-review.json)和独立[方法映射表](validation/mi26-blind-review.key.json)，不向审阅者透露方法、分数和参考答案。尚未取得独立人工票，不能声称人工质量验证完成。本任务要求的判定方法、可复现对照、编辑前文及连续链覆盖已齐备；不要求用优选演示造出更高准确率，也不将坏模型的评测结论伪装成达标模型。

结论仍是当前小模型/策略仅适合实验，不能根据一次参考命中或推理提速宣称预测质量改善；允许继续下载/配置机制开发。新保留集现已使用，后续参数选择不得再使用它。

```powershell
python model_input/tools/prepare_generation_evaluation.py --offset 24 --count 32 --edited-contexts --output model_input/evaluation/generation-acceptance.json
target/release/evaluate-generation.exe MODEL.gguf model_input/evaluation/generation-policy.json model_input/evaluation/generation-dev.json > dev-runs.json
target/release/evaluate-generation.exe MODEL.gguf model_input/evaluation/generation-policy.json model_input/evaluation/generation-acceptance.json model_input/evaluation/generation-acceptance-plan.json > holdout-runs.json
python model_input/tools/report_generation_evaluation.py model_input/evaluation/generation-dev.json dev-runs.json model_input/evaluation/generation-acceptance.json holdout-runs.json report.json --plan model_input/evaluation/generation-acceptance-plan.json --review-sheet blind-review.json
```

冻结输入及带源文件哈希的 MI-26 JSON 证据由 `.gitattributes` 禁用换行转换；数据生成器固定输出原冻结文件使用的 CRLF，跨平台重新生成不会仅因换行改变 SHA-256。已重新生成到独立目标并核对完整文件哈希，模型参数和原始评测输入字节均未改变。
