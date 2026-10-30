# 输入会话与资源协议 v1

CP-11。C 头文件为 `core_engine/include/mywubi.h`；Rust 与 JNI 共同调用
`core_engine::ffi::call`，不复制状态机。旧 `mywubi_user_dict_*` 裸指针导出保持原契约，
必须传有效指针且仅销毁一次，不能与新整数句柄互换。

请求为标准 UTF-8 JSON：`{"version":1,"request":{"op":"State","session":6}}`。
返回 `{"version":1,"ok":true,"value":...}`，或 `ok:false,error:{code,message}`。
字符串长度按 UTF-8 字节计，允许嵌入 NUL（JSON 中转义），非 BMP 字符按 Unicode
标量传递；具体词条仍执行共享校验。请求最大 16 MiB，字面文本最大 65536 字节，
候选窗口最大 256。未知版本、动作、枚举、非法 UTF-8 和不完整字段均拒绝。

错误码：1 版本不匹配；2 请求非法；3 句柄无效/种类错误/已销毁；4 Busy；
5 数据读写或校验失败；6 文件版本冲突；7 会话拒绝/关闭/需要清理；8 内部失败。
会话错误具体原因在本次响应内；没有全局 last_error。

句柄低两位区分快照、会话、文件版本，高位为永不复用的进程内代次。Destroy 后
旧代次不会指向新对象。文件版本还绑定读取路径；另存前须对目标调用 FileVersion。
进程内调用串行化，调用结束释放锁后宿主才执行编辑计划；输入无逐键读表。
版本句柄也须 Destroy；快照可在 CreateSession 后销毁，会话保有资源引用。

## 输入操作

| op | 输入字段 | value |
| --- | --- | --- |
| BuildRuntime | path（配置文件） | handle（严格加载的 TouchV1 快照） |
| CreateSession | runtime, context | handle |
| Prepare | session, action | token/op/target/preview/disposition |
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

包激活顺序：ValidatePackage → 保存配置 → BuildRuntime → ApplyRuntime；保存与应用是
两个独立结果。Android 的崩溃恢复式活动包切换属于 CP-19，不由任意路径写入代替。

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
