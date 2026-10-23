# 跨端方案第二轮审核与修订记录

日期：2026-10-23。范围：[职责与接口方案](cross-platform-core-design.md)、[实施计划](cross-platform-implementation-plan.md)、[ROADMAP](../ROADMAP.md)，并同步 [现状记录](android-readiness.md)。

本轮审核针对设计，不是已实现代码的交付验收。对照当前 Rust/Windows 调用链，发现并直接修正以下 14 类遗漏或兼容风险；没有修改代码、安装工具或变更 AGENTS.md。

## 1. 发现与处理

| 编号 | 原方案问题 | 已写入的修正 | 验收对应 |
| --- | --- | --- | --- |
| D01 | “编辑成功后才切换/发布”可能统一改变 Windows 开关、状态机和运行时的可观察时点；临时失败回滚描述过宽 | 区分处理状态与已发布候选；LegacyV1 保留编辑前推进、仅 handle 有 rollback，切换/失焦/重载不自动回滚；TouchV1 才采用主动切换待成功 | W16、W22，CP-03/08/09 |
| D02 | 把整个 TSF 预检理解为纯查询，或用新编辑计划拆分同步请求，会改变清理/Shift/运行时同步和撤销时序 | 纯查询要求只约束核心；保留外层准备和 TF_ES_SYNC，请求与执行 HRESULT 均检查；顶字保持同一 edit session | W21，CP-02/08/09 |
| D03 | 没有公开 Recover/关闭接口，Busy、待确认与控制事件关系不完整，可能永久卡住或掉键；单 pending 也会改变旧 Windows 重入 | 增加处理阶段、恢复/关闭、控制事件优先级和手机有界顺序队列；LegacyV1 保留同线程重入及 generation 作废，不用 Busy 吞/透传；未知结果清理需所有权 | W22、A10/A18、B04，CP-08/18 |
| D04 | 只有 SetPreedit(text) 不足以保留 Windows 显示属性；None、失败视图和 anchor 处理不明 | 增加 kind、视图不变与发布时点，保留 Input/Converted 和原 anchor 回退，计划预览不直接显示 | W23，CP-04/09 |
| D05 | 将手机数字/符号面板一律当字面输入，会绕过自定义 charset 和万能键 | 增加由核心路由的 InputText/intent；主表声明字符优先编码，强制字面意图才绕过；不重排旧 Windows key_filter | W01/W05、A06，CP-02/16 |
| D06 | “空回车执行宿主动作”未排除空临时模式；重复进入和不可用目标可能先提交旧缓冲 | 空临时空格/回车消费但不发送；先验证目标、重复请求幂等，取消/切临时模式和返回模式明确 | A16，CP-03/16/17 |
| D07 | 统一配置/资源验证与后台初始化可能收紧旧加载条件，或使 Windows 第一批键在未就绪状态运行 | 区分读取/保存/活动资源校验、空引用/损坏引用，保持 Windows 初始化触发和各失败回退；新 Android 初始化及重载异步 | W17、W24、R01，CP-05/06 |
| D08 | “统一路径解析”过于笼统，可能使用宿主 exe 或重定向 AppData | 明确设置 exe、TSF DLL 与未重定向 Roaming 的区别，保留便携优先与 ACL 职责，Android 显式提供私有目录 | W19/W20，CP-05/09 |
| D09 | 资源包定义和持久化激活顺序缺失；内存发布成功不能保证重启后正确；版本检查可能被误认为跨进程原子 CAS | 明确主表+直接引用集合、版本目录、持久化活动配置切换/重启恢复、存活资源保留；限制写序列的保证范围，不默认改变 Windows 写策略 | R02/R04/R06，CP-07/19 |
| D10 | 对旧裸指针 ABI 也承诺安全拒绝重复销毁，会引入 ABI 改动或非法测试；资源桥接未列依赖 | 新代次句柄与旧指针契约隔离；CP-11 增加 CP-07 依赖，覆盖数据与恢复接口，增加跨 Rust/C/JNI 等价样例和逐调用错误归属 | B02/B06，CP-11/13 |
| D11 | 未表达不支持 composition 的宿主能力，且 View 重建与连接更换界限不清 | 输入上下文增加能力和所有权；明确本地预编辑的进入条件，禁止在未知提交结果后改用直接插入重试；View 与会话分别管理 | A09/A17/A18，CP-14/17/18 |
| D12 | Windows 在 CP-10 验收后，后续 Android 共享变更可能绕过回归，最终拿旧制品证据结项 | G2 改为持续条件，后续共享改动继续自动化，CP-24 对最终制品重新真实 TSF 验收 | G2/G5，CP-24 |
| D13 | 一个 revision 同时承担输入状态、查询和翻页，可能让滚动使所有候选失效，或让旧输入仍可点击 | 分开 generation、query_revision、view_revision；只读窗口与旧页内编号映射明确 | W02、A02、B04，CP-04 |
| D14 | 将菜单称为前端展示后，没有对应的进入/选择/关闭及消费状态；还可能把所有主表限制成 Shape | 补选择器兼容动作和非文本状态，保留旧 Menu 表现；合法主表 type:pinyin 仍按旧 Main 路径工作 | W01/W10/W13，CP-02/03 |

## 2. 核对依据

- [Windows 输入会话](../windows/im_engine/src/input_session.rs)：handle、set_chinese、lose_focus、reload、recover、execute、plan 的状态更新、回滚与视图映射。
- [TSF 文本服务](../windows/im_engine/src/text_service.rs)：prepare_context、translated_key、OnTestKeyDown、RequestEditSession、apply_ime_mode、composition range。
- [按键适配](../windows/im_engine/src/key_filter.rs)：模式语法、快捷键、翻页、ToUnicode 与标点转换顺序。
- [核心状态机](../core_engine/src/state_machine.rs)、[临时输入](../core_engine/src/state_machine/temporary_input.rs)：编码/选择、空模式动作、菜单及 reset 规则。
- [配置](../core_engine/src/config.rs)、[辅助资源加载](../core_engine/src/temporary.rs)、[码表编辑保存](../windows/settings/src/table_session.rs)：加载和保存的校验差异与文件保存顺序。
- [TSF 配置路径](../windows/im_engine/src/config_path.rs)、[共享路径](../core_engine/src/config_path.rs)、[设置路径](../windows/settings/src/config_path.rs)：DLL/exe、Roaming 与便携处理。
- [用户词库 ABI](../core_engine/src/user_dictionary.rs)：旧 open/destroy 的裸指针安全契约。

## 3. 修订后状态与验证边界

- ROADMAP 仍为 24 项 CP 任务；修改了验收内容和 CP-11 的依赖，没有将待实施项勾选完成。
- 矩阵扩展为 54 类：Windows 24、Android 18、资源 6、桥接 6；新增场景是未来必须执行的验收，不是本轮已经通过的测试。
- 原有皮肤、词频规划和历史已完成部分保持不变。
- 本轮检查本地链接、任务/依赖一致性、矩阵引用、Markdown 结构与改动范围。业务代码未改，不重复执行上一轮的 233 项业务测试；该数量仍只作为先前实测基线。
- 设计遗漏已按本轮发现修订；最终无回归结论仍须在实现后由差分轨迹、故障注入和最终版本真实宿主测试证明。工具版本/最低 Android 版本、设备性能阈值继续由前置构建及设备基线任务确定，不能在未验证时当成已定事实。
