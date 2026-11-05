# MI-26 生成式预测对照（2026-11-05）

当前 Qwen2.5-0.5B Q4_K_M + `qwen25-short-words-v1` 仅适合实验，不推荐启用。
保留集没有精确参考命中；1000 ms 下只有 1/33 样本显示结果。不能声称准确性提升。

## 数据与冻结

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

## 全部结果

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

MI-26 工具、来源隔离及初次对照已完成；编辑前文的独立质量样本、多步成功链和
独立人工复核仍不足，整项暂不勾选。可以继续下载/配置机制开发，不能把此模型
设为通过质量验收的推荐项。后续调参需新版本及新的未触碰保留集。

## 复现

```powershell
python model_input/tools/prepare_generation_evaluation.py
cargo build --release -p model_input --features llama-cpp-2 --bin evaluate-generation
target/release/evaluate-generation.exe MODEL.gguf model_input/evaluation/generation-policy.json model_input/evaluation/generation-dev.json > dev-runs.json
target/release/evaluate-generation.exe MODEL.gguf model_input/evaluation/generation-policy.json model_input/evaluation/generation-holdout.json > holdout-runs.json
python model_input/tools/report_generation_evaluation.py model_input/evaluation/generation-dev.json dev-runs.json model_input/evaluation/generation-holdout.json holdout-runs.json report.json
python -m unittest discover -s model_input/tools -p test_generation_evaluation.py -v
```
