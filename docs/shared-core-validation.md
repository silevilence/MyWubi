# 共享核心 CP-01～CP-09 验收记录

日期：2026-10-27。范围为本次指定的九项 ROADMAP 任务，任务完成标记原地更新。
CP-10 真实 TSF 验收以及 CP-11 之后的 C-ABI/JNI/Android 工程不在本次范围。

## 实现与证据

| 任务 | 实现落点 | 自动化证据 |
| --- | --- | --- |
| CP-01 | [Windows 基线](windows-compatibility-baseline.md)；并发临时目录隔离 | W01～W24 映射；嵌套输入、控制事件、失败、预判和视图轨迹 |
| CP-02 | `state_machine/semantic.rs`：Action、TextIntent、Disposition | `semantic_events`：数字编码/字面意图、符号/万能键、旧事件差分 |
| CP-03 | BaseMode、TemporaryMode、Capability；按配置组加载辅助表 | `semantic_modes`、`temporary_loading`、`session_contract`：无入口键、重复请求、空临时回车、返回模式、确认失败 |
| CP-04 | `candidates.rs`；版本化窗口/候选身份；结构化提示和预编辑类别 | `candidate_contract`：翻页身份稳定、跨会话/旧版本拒绝、窗口上限、提示与正文分离 |
| CP-05 | Config 内存投影与 ValidationContext；StorageContext | `config_projection` 及原路径/配置测试：旧字段往返、桌面热键场景、显式私有目录 |
| CP-06 | `runtime.rs`：RuntimeBuilder、RuntimeSnapshot、RuntimeStore | `runtime_contract`：权重覆盖、反查隔离、故障依赖、启动回退和迟到结果拒绝 |
| CP-07 | `data.rs`：校验、另存、受管写入、文件版本与词条身份 | `data_contract` 和设置程序原测试：版本冲突、部分成功、无效草稿保留、引用重定位和包边界 |
| CP-08 | `session.rs`：prepare/acknowledge、恢复/关闭、宿主能力与所有权 | `session_contract`：Busy、过期 token、嵌套处理、预览不发布、不重放、运行时更新与本地预编辑 |
| CP-09 | Windows 资源装配、InputSession 薄适配与设置保存 | 旧驱动差分、全部原 Windows 测试及完整工作区构建 |

## Windows 迁移与回退边界

资源装配单独提交 `333cf55`，通过完整工作区回归后再接入共享会话/视图/保存。
会话适配提交 `2c61b77`。生产路径只调用共享 Session 一次，TSF 宿主编辑保持同步且在锁外执行；顶字仍为单个 CommitThenUpdate/edit session。

`text_service.rs`、`key_filter.rs`、GDI 候选窗口、DLL 路径发现、TIP/COM 和依赖锁文件未改变。
英文旁路继续由 input gate 表达，持续英文能力不会自动打开 Windows 英文候选。

`windows/im_engine/tests/support/legacy_input_session.rs` 是 CP-01 提交 `2012667` 的冻结旧驱动，仅在 `cfg(test)` 下编译。
差分测试比较消费结果、编辑操作序列、模式、候选/提示/分页、整个 CandidateData（包括 anchor）及恢复结果；覆盖八组初态、三种编辑结果和输入/控制序列。
已有嵌套输入与控制事件测试继续保留原断言。None 保留旧视图；控制事件的 Reset 清除平台旧锚点。

## 接口使用约束

- 资源加载先由 RuntimeBuilder 构建，再由 RuntimeStore 或平台发布；会话采用和宿主清理是后续阶段。按键派发不读文件、不建索引。
- 旧 Windows 调用使用 Legacy 事件映射。新字符入口传 InputText 的 Normal/Literal 意图；选词使用查询版本与 CandidateId。
- CandidateWindow 不改桌面页大小。窗口查询沿用完整检索/排序，只复制有上限的返回窗口；Windows 按键路径不额外查询整批候选。这里没有宣称渐进检索或新的端到端性能测量。
- TouchV1 的 prepare 返回未发布预览。宿主执行前校验 token，执行后 acknowledge；Busy 必须由平台保留在有界有序队列中，不当作 NotHandled。
- LegacyV1 在编辑前推进处理状态，允许同线程重入；只有最新 generation 能发布。临时 handle 的确认未写入可回滚，切换/重载等控制操作不套用该回滚。
- TouchV1 的不确定/部分成功结果默认放弃预编辑所有权。适配层只有确认仍拥有同一范围时才调用 acknowledge_with_ownership；Recover 只清理，不重放提交。
- 无预编辑能力但支持插入的宿主采用本地预编辑，最终提交当前选区。仍有效的旧连接失焦或运行时更新时，基础缓冲提交原文；断开连接使用 invalidate_context，仅作废本地状态。
- acknowledge 可返回延后的最新运行时清理计划 follow_up，调用者继续执行并确认。换宿主/关闭可以打断等待；仅 View 重建且连接连续时不调用 invalidate_context。
- HostContext 传入输入用途、插入/预编辑能力、选择/周边文本能力及回车动作。受限输入场景只接受字面文本，功能键交还宿主，模式请求不会改变基础模式。
- 配置磁盘格式保持原样；手机 UI 偏好独立保存，不复制引擎配置。保留无效旧字段及其诊断，不在这次重构中启用。
- Expected 文件版本按内容检测冲突，在本进程受管写入序列中检查并写入；不承诺对外部编辑器提供原子 CAS。旧 Windows 选择 LegacyOverwrite。
- 每个文件独立原子替换，多文件用 SaveReport 报告已保存部分。导入包边界检查已共享；Android 目录激活/崩溃恢复仍由后续 CP-19 实现。

## 最终验证

Windows 主机，Rust 1.99.1；依赖均使用现有锁文件与离线缓存。

| 检查 | 结果 |
| --- | --- |
| `cargo test --workspace --quiet --locked --offline` | 274 项通过，0 失败，0 忽略 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy -p core_engine --all-targets --locked --offline -- -D warnings` | 通过 |
| `cargo build --workspace --quiet --locked --offline` | 通过 |
| `git diff --check` | 通过 |

核心 175 项、Windows 输入法 72 项（69 单元 + 3 集成）、设置 21 项、TIP 管理器 6 项。
本地完整测试输出保留在忽略目录 `target/shared-core-validation.log`。

## 整体审核修复与边界

审核补齐了临时轮次清空与退出的区别、未知结果后的所有权重确认、延后运行时在换宿主后的采用、受限字段的模式保护、启动失败分支不重新加载辅助表，以及无宿主预编辑范围时的原文收尾。
同时移除了新候选结构在 Windows 每次按键上的额外全量检索/整批文本复制；所有修改都有对应契约或差分回归。

现有 DLL 链接 LNK4104 警告仍存在；没有全局放宽 lint，也没有将历史工作区严格 Clippy 问题写成已修复。
本次没有注册新 DLL、在真实宿主逐键输入、执行 Velopack 发布或 Android 构建。记事本/浏览器、撤销、语言栏和 GDI 实机检查仍由 CP-10 验收，历史记录不能代替新制品证据。
Windows fallback 的 Rust scalar 长度风险仍记录在基线，不用于 TouchV1 的删除范围。
