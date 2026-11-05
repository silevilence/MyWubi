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
