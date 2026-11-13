# 特殊输入定向实施记录

## 当前状态：2026-11-13

首次收尾后又完成代码审核与修复：修正预览撤回卡住、预测设置热重载保留旧时间快照、试用查询无法退格、保存错误未定位字段和窄窗 Esc 返回遗漏。修复后统一测试 504 项通过、6 项既有条件测试忽略；详见[逐条审核记录](2026-11-13/RESULTS.md#收尾后的审核与修复)。以下首次收尾制品和性能数据仍对应审核前提交，不含本次补丁；实机状态不变。

SI-01～SI-04、SI-06～SI-09 已完成；SI-05 和 SI-10～SI-15 的实现及可在后台完成的检查已推进，整体仍受各自真实环境验收条件约束。SI-16 仅完成构建、命令行和集中验收准备；SI-17 的设备测试已编译，尚未执行。下方 11 月 12 日条目是历史阶段记录，其中“未开始”“未接入”等描述由本节及 ROADMAP 后续证据更新，不代表当前状态。

- 双端已实现统一特殊输入配置、六类资源管理、两种查询方式、日期编码/全局位置/格式、共享预览及 `.mwsi` 备份恢复。用法见 [使用说明](../../special-input-guide.md)。
- 统一 Workspace 检查：498 项通过、6 项既有条件测试忽略；其后新增 Windows 全局位置适配层测试单独通过。桌面 JVM 真实 JNI 10 组、1168 次请求通过，不能替代 Android 设备或真实宿主。
- Windows 核心三轮性能门槛全部通过，峰值最大 114.7 MiB；完整记录、失败过程、设备缺口见 [自动化与性能记录](2026-11-13/RESULTS.md)。
- Windows 本地验收 Setup/Portable/nupkg、Android 双 ABI Debug APK 和设备测试 APK 已构建；包内 22 项资源/许可及应用二进制核对通过，见 [制品摘要](2026-11-13/artifacts.json)。未安装、升级或发布。
- 全程复用本地资源，零下载。只用 cua 读取能力/窗口清单，未点击、输入或切换焦点；Windows 输入会自动激活窗口，Android 无连接设备，按用户要求暂缓交互。下次按 [集中实机清单](host-checklist.md) 一次完成取证，不能把清单当成通过记录。

## 2026-11-12，SI-01

设计与数值门槛见 [设计契约](../../special-input-design.md)。`cases.json` 是固定预期，不能作为实现通过证据。`baseline-windows.json` 是旧核心路径三轮 Release 原始测量，`baseline-memory.json` 是独立进程工作集采样；复用本地资源，未联网下载。基准工具定向严格 Clippy 通过。Android 历史参考与当前无设备边界在设计文档中单列。

## 2026-11-12，SI-02

- 六类资源支持各自选择、默认关闭、独立加载、受控导入和错误隔离，旧 Emoji/颜文字序列化保持原名字。
- 配置、共享协议能力、候选来源及模式名称覆盖新类别；Windows/Android 配置读写同时扩展，界面统一管理仍属 SI-10/12。
- `cargo test --offline -p core_engine --test expression_resources --test expression_store --test expression_candidates --test expression_query --test expression_search`：最终对应 52 项测试通过。新增测试验证六类配置往返、错误资源不覆盖文件、协议导入不激活，以及两个行为配置组的新类别身份/显示名称。
- `cargo test --offline -p settings --lib expression_catalog`：1 项通过，导入结束不会覆盖已经重新选择的资源。
- `cargo clippy --offline -p core_engine --lib -- -D warnings` 通过；`cargo check --offline -p im_engine -p settings -p mywubi_android` 通过。
- Android 使用本地 JDK 21.0.8 / SDK 缓存执行 `:app:compileDebugKotlin --offline -x cargoBuild` 通过。此项仅验证 Kotlin 编译，未生成最终 APK、未替代 ARM64/JNI 设备验证。SDK package.xml 写入警告未阻断离线编译，未修改 SDK 权限。
- Windows settings/im_engine 的全量严格 Clippy 仍有原有告警：app.rs 的 collapsible_if、model_catalog 类型复杂度、dictionary 面板旧风格及 factory.rs 原始指针接口。此次新增 needless_borrow 已修复；未修改无关代码或关闭 lint，不能称整个 Windows workspace 严格 Clippy 已通过。

本阶段不宣称 SI-05 的字面检索和实际菜单、SI-10～SI-13 的统一管理界面、SI-16/17 的真实宿主验收已经完成。

## 2026-11-12，SI-03

`cargo test --offline -p core_engine --test special_datasets` 两项通过，覆盖 224 项清单、全部读音别名的大小写/稳定精确排序、组合形式往返及独立 `shi → し/シ` 样例。生成两次 SHA-256 相同。共享模糊查询允许明确标记的部分字符匹配，不能把“不是罗马字转写”误写成必须零结果；测试已据此核对不会合成日语词语。

## 2026-11-12，SI-04

数学 124 项、标点 49 项、其他符号 109 项；`special_datasets` 最终 3 项通过。检查所有条目的完整名称触发和唯一输出，代表查询命中及排序正确，多字符输出和形近/单位形式原样往返。四个新库重复生成均逐字节一致，SHA-256 与清单匹配，未下载第三方资源。

## 2026-11-12，SI-05（部分完成）

- 新增 `ExpressionInput::{Words,Literal}`、切换/清空动作及本地字面查询缓冲。512 字符上限、按字素退格、非空拒绝切换、结果数字选词均由核心负责；查询不作为宿主预编辑或上屏文本。
- Windows 辅助菜单包含拼音、英文及六类特殊输入；操作区提供切换、清空、查找、修改和取消。Android 使用单个“特殊输入”菜单列出六类及禁用原因，避免继续扩大工具栏。
- `expression_query` 15 项、`expression_candidates` 18 项、`temporary_input` 9 项、`special_datasets` 3 项通过；覆盖 SI-C10 的核心轨迹、六类顺序和跨类别去重，旧分组分页保持不变。
- `cargo test --offline -p im_engine --lib input_session::tests` 27 项通过；这是适配层模拟编辑器验证，不是实际 TSF 宿主证据。
- 核心严格 Clippy 通过。Android `:app:compileDebugKotlin` 与 `:app:compileDebugAndroidTestKotlin` 离线通过（均 `-x cargoBuild`）；已有界面用例同步新菜单，新增 `ExpressionHostTest.kanaLiteralAliasUsesVisibleCategoryEntryAndCommitsOnlyTheResult` 待设备运行。
- 四个新资源、说明和覆盖清单加入共用 bundle 清单及 Android 首次复制流程；完整升级、便携与 APK 分发验收未执行。未连接 Android 设备，未执行本次真实 UI 验收，不能把以上编译/模拟证据记作 SI-16/17 通过。

## 2026-11-12，SI-06（规则基础，尚未接入配置）

- `shortcuts` 定义规则、类型、全局位置和共享上限；验证成功才返回小写规范副本，失败不修改草稿，错误包含原始行号和字段。停用项参与总数、同码数量和重复位置检查。
- `shortcut_format` 编译六种固定格式标记和单引号字面量，拒绝未知 ASCII 字母标记/未闭合引号。此处只有语法树，尚无取时、日期渲染或时区实现，不属于 SI-07 完成证据。
- `ShortcutEnvironment` 可从主表配置投影；冲突状态与启用意图区分，字母不区分 ASCII 大小写。未要求对应词条或主表码长满足快捷编码。
- `cargo test --offline -p core_engine --test shortcut_rules` 最终 7 项通过，覆盖所有约定结构边界、停用重复项、原草稿保留、规范化、Unicode 标量长度、格式语法、冲突暂停与恢复及真实主表配置投影。
- **接续工作**：先把规则接入 `Config` 的反序列化、规范化读写和保存校验，同时更新 Windows/Android 草稿编辑与协议。再完成有效规则集合及错误展示；格式语法已准备好，但 SI-07 的平台时间提供者、冻结快照和格式化输出仍未开始。当前不向用户暴露尚不能生效的日期规则开关。

## 16:00 截止的阶段收尾

- SI-01～SI-04 已完成各自实现、定向检查并分别本地提交；SI-05 的入口/查询/追加已实现，但整体验收仍待真实宿主及 SI-08；SI-06 只完成共享规则与语法基础。SI-07～SI-17 未完成，日期候选和统一设置界面尚未交付。
- 集中执行 `cargo test --offline -p core_engine -p im_engine --lib --tests` 通过；Windows 输入层 78 项通过、1 项既有真实模型服务测试按原条件忽略，候选数据集成 3 项通过。`cargo test --offline -p settings --lib` 23 项通过。
- 集中审核修正了六类资源热重载等价比较遗漏、字面 `InputCode` 的预判不一致，以及 Android 拼音锁定在新类别查询中错误尝试恢复的问题；设置文案同步新入口。最后定向重跑 `expression_candidates` 19 项、`expression_query` 15 项、`shortcut_rules` 7 项及核心严格 Clippy 全部通过。Android 主程序/设备测试 Kotlin 再编译通过；未把 Kotlin 编译写成 JNI 真机验证。
- [bundle-check.json](bundle-check.json) 记录 22 项本地资源/许可文件：Windows 分发复制脚本输出与 Android 生成 assets 均与源文件 SHA-256 相同。复用本地资源，零下载；这不是 Setup/Portable/APK 的最终交付验收。固定验收样例和压力样本摘要再次核对未改变。
- 尚无本轮真实 TSF/Android 宿主、可访问性、最终制品和新功能性能证据；不勾选 SI-16/17，不推送、不发布。下一轮从 SI-06 的配置及双端保存衔接继续，随后 SI-07～SI-09；统一设置按 SI-10～SI-13 的已定设计实施，最终集中完成真实环境验收。
