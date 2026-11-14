# 输入会话与资源协议 v1

CP-11。C 头文件为 `core_engine/include/mywubi.h`；Rust 与 JNI 共同调用
`core_engine::ffi::call`，不复制状态机；JNI 通过 `call_with_clock` 为新会话注入平台时钟。旧 `mywubi_user_dict_*` 裸指针导出保持原契约，
必须传有效指针且仅销毁一次，不能与新整数句柄互换。

请求为标准 UTF-8 JSON：`{"version":1,"request":{"op":"State","session":6}}`。
返回 `{"version":1,"ok":true,"value":...}`，或 `ok:false,error:{code,message}`。
字符串长度按 UTF-8 字节计，允许嵌入 NUL（JSON 中转义），非 BMP 字符按 Unicode
标量传递；具体词条仍执行共享校验。请求最大 16 MiB，字面文本最大 65536 字节，
候选窗口最大 256。未知版本、动作、枚举、非法 UTF-8 和不完整字段均拒绝。

错误码：1 版本不匹配；2 请求非法；3 句柄无效/种类错误/已销毁；4 Busy；
5 数据读写或校验失败；6 文件版本冲突；7 会话拒绝/关闭/需要清理；8 内部失败。
会话错误具体原因在本次响应内；没有全局 last_error。

句柄低两位区分快照（1）、会话（2）、文件版本（3），响应缓冲使用独立的 0 种类，
跨种类释放会被拒绝。高位为永不复用的进程内代次。Destroy 后
旧代次不会指向新对象。文件版本还绑定读取路径；另存前须对目标调用 FileVersion。
进程内调用串行化，调用结束释放锁后宿主才执行编辑计划；输入无逐键读表。
版本句柄也须 Destroy；快照可在 CreateSession 后销毁，会话保有资源引用。

## 输入操作

| op | 输入字段 | value |
| --- | --- | --- |
| BuildRuntime | path（配置文件） | handle（严格加载的 TouchV1 快照） |
| CreateSession | runtime, context | handle |
| Prepare | session, action | token/op/target/preview/disposition/learnable |
| Ack | session, token, outcome, owns_preedit | status, follow_up |
| Recover / LoseFocus | session | 可选或必选编辑计划 |
| Close / Destroy | session / handle | null |
| State | session | status/view/base/temporary/revision/pinyin 能力 |
| Candidates | session, offset, count | 版本化候选窗口 |
| IsCurrent | session, token | bool |
| Context | session, context, preserve_base | null（旧连接失效，不写新选区） |
| ApplyRuntime | session, runtime | 可选清理计划；拒绝旧 revision 覆盖 |

context 的字段对应 `HostContext`；调用方明确提供 editor/epoch、预编辑/插入能力、
所有权、输入用途、选区/上下文能力与回车动作。action 使用 Serde 外部标签，
如 `{"InputText":["a","Normal"]}`、`"Space"`、`{"SetBaseMode":"English"}`、
`{"BeginTemporary":"Pinyin"}`、`{"SelectCandidate":[{"engine":1,"version":2},0]}`。
完整标签由 `Action` 定义，Legacy 在此 TouchV1 接口拒绝。

Prepare 的 preview 不得提前显示。执行前检查 IsCurrent，执行后用原 token 确认；
不得将未知结果当作未写入再提交。HostOutcome 支持 Applied、RejectedBeforeWrite、
PartiallyApplied{completed_steps}、OutcomeUnknown、StaleContext；follow_up 同样执行并确认。
replace_len 是 Rust 标量数，不能直接当 UTF-16 下标。Android 使用 owned composition
替换语义，不按该数值删除周边文本。关闭会话作废计划，但不会通过失效连接写宿主。

## 资源操作

| op | 输入字段 | 输出 |
| --- | --- | --- |
| ReadConfig / ReadTable / ReadUser | path | 配置/元数据词条/词库及 version |
| ReadTableConfig | path | 仅 YAML 头配置（含 display_name），不返回词条或 version 句柄 |
| FileVersion | path | version（目标不存在也可获取） |
| ValidateConfig | config | null 或校验错误，TouchSave 场景 |
| ValidateTable | config, entries | 结构化校验报告 |
| SaveConfig | path, config, expected | 新 version |
| SaveTable | original, target, config, entries, expected | 重定位后的 config、新 version |
| ValidatePackage | root, main | 完整直接引用路径集合 |
| EditUser | path, expected, edit | added/updated/version |
| ExportUser | source, target, expected | version |
| SaveBundle | path, config, expected, table（可空） | saved 路径和 failure |

EditUser 标签为 Add(Entry)、Update([UserEntryId,Entry])、Remove(UserEntryId)、Import(path)。
SaveBundle.table 包含 original/target/config/entries/expected；failure 非空时必须展示部分成功，
不能仅凭传输 ok 判断整体保存成功。文件版本保存了读取时的精确内容；应用内写入串行检查，
不承诺与任意外部编辑器做文件系统原子 CAS。

Android 包激活顺序：复制独立包 → 显式重绑定 → ValidatePackage → 临时配置试建
BuildRuntime → 原子保存活动配置 → 通知服务 BuildRuntime/ApplyRuntime。保存与应用
是两个独立结果；旧包继续保留，切换前后崩溃均能从磁盘活动配置重新加载。
BuildRuntime 的文件读取与引擎构建在句柄注册表锁外执行，成功后仅加锁登记句柄；
服务用加载代次拒绝迟到结果。会话调用仍串行，宿主回调期间不持有注册表锁。

## C 内存与验证

C 调用返回独立 `MyWubiBuffer`，data/len 不依赖下一次调用。必须用 allocation 调用
`mywubi_v1_buffer_free`；重复释放返回 3。输入/输出非空指针仍须由调用方保证内存有效，
不能检测任意野指针。捕获可展开 panic，不承诺捕获 abort/OOM。

主机验证：`cargo test -p core_engine --test ffi_protocol`；Windows C 冒烟：

```powershell
cargo build -p core_engine
clang core_engine/tests/abi_smoke.c target/debug/core_engine.dll.lib -o target/debug/abi_smoke.exe
./target/debug/abi_smoke.exe
```

2026-10-30：Rust/C 传输等价、编辑确认、Busy、跨会话 token、候选身份、未知结果恢复、
关闭/销毁、资源冲突、非 BMP/NUL/组合字符测试通过；独立 C 程序成功链接调用。
JNI 使用同一 facade，设备等价验收记录归 CP-13。


## 特殊输入设置操作（SI）

这些操作在设置工作线程调用。预览、备份和恢复的文件读取在会话注册表锁外执行，不占用活动会话锁。

| op | 输入字段 | value / 行为 |
| --- | --- | --- |
| InspectShortcuts | rules, table | 规范化 rules、逐条 states（用户启用与环境可用分开） |
| PreviewShortcuts | path, config（草稿）, code, time（LocalTime） | rows、total、effective_capacity、states、warnings、resources；固定时间由调用者标明 |
| PreviewExpression | path, config（草稿）, kind, input（Words/Literal）, actions | view、window、selected、resources；仅试用，不写宿主 |
| ExportSpecialInput | path, config（草稿）, target | exported:true；原子生成 `.mwsi`，不保存配置 |
| RestoreSpecialInput | path, config（当前草稿）, source | config（替换特殊输入后的新草稿）；资源写入新导入目录，不保存配置 |

`.mwsi` 版本 1 包含六类所选资源原文、开关和快捷规则，不包含主码表/个人数据或环境暂停状态。缺失/重复类别、损坏资源和非法规则在创建导入资源前拒绝；保存草稿仍须调用带版本检查的 SaveConfig。调用方应防止异步恢复覆盖更新的草稿。备份文件通过路径传递，上限 512 MiB，不放入 16 MiB 协议请求；每个资源仍受 64 MiB 限制。

日期候选的提示为 `DateTime(rule_index)`，保留来源身份；`PreparedEdit.learnable=false` 的提交须隔离个人采集/预测上下文，不得当普通词训练。`SessionView.shortcut_error` 为可选的本轮取时错误。每页/连续窗口仍使用 QueryRevision 和 CandidateId，不能用显示文本作为候选身份。

## 字符组协议（AK-06）

`KeyboardCapabilities` 无需句柄，返回组协议版本、32 位置/32 字符组上限和布局/方向能力。现有单字符动作与 envelope 版本保持不变。`BindMapping(identity, definition)` 在空会话绑定映射，identity 为 id/version/revision，definition 为 charset/groups；只读声明数据，不执行脚本。`InputGroup(identity, group_id)` 输入一个位置，版本不符拒绝。空集、重复、超限、控制字符或不属于当前码表的字符拒绝，错误不改变输入。

`ResolveGroup(query_revision, position, character)` 明确一个位置；`UndoGroupResolution(query_revision)` 撤回约束。`State.groups` 返回原始 positions、selected 和映射身份；候选剪枝不改变 positions。提示仅显示于键盘，不能写宿主 preedit。未确定回车/原文提交拒绝并提示先选词/消歧；失焦不把组标记当原文提交。密码及 Direct 在 Session 层拒绝组动作。

`groups.json` 由 Rust 和 Android JNI 测试共同消费。组检索及上屏策略分别在 AK-07/08 扩展，不以协议测试替代最终真实宿主验收。
