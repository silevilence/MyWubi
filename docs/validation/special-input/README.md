# 特殊输入定向实施记录

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
