# EM 验收记录

日期：2026-11-09。按 ROADMAP EM-01～EM-12 原范围推进，未通过项保持未完成。

## EM-01 共享配置与资源契约

`core_engine/tests/expression_resources.rs` 覆盖：

- 旧配置缺省关闭，两类自动追加/专用模式独立，主码表改选后配置往返保持。
- 多触发词去重、完整词严格匹配、UTF-8 输出保存/重载无损（ZWJ、肤色、VS16、反斜杠、全角空格、组合附加符）。
- 改为 `.dict` 后，列表头读取、编辑读取、核心加载、配置保存均拒绝；桌面和触屏运行时结果一致。
- 普通码表、错误类型、无来源、控制字符和多行输出拒绝。
- 两用途关闭时缺失资源不读取；启动隔离错误、另一类词库与主输入仍有效；失败热重载保持旧快照，修复后发布。
- 已启用词库损坏或误选普通码表时拒绝保存，原配置字节不变；关闭后可保存。

Windows 设置列表另有 `main_table_choices_exclude_renamed_expression_resources` 回归测试。双端设置同步增加软件全局字段；完整导入、加载状态和可用交互仍由 EM-05/EM-11 验收。

共享核心 lib/tests 通过；7 个表情契约测试通过。Windows `cargo check -p settings -p im_engine --all-targets --locked --offline` 通过，主表列表排除回归通过。`cargo clippy -p core_engine --all-targets --all-features --locked --offline -- -D warnings` 通过。

Android `:app:compileDebugKotlin --offline --console=plain` 通过，包括 arm64-v8a/x86_64 的 JNI 构建。SDK 位于只读目录产生 package.xml 写入警告，但构建成功；没有修改 SDK 权限。此结果证明配置界面与共享协议可编译，不替代设备显示验收。

全量 Windows Clippy `-D warnings` 发现既有 Win32 COM trait 原始指针及绘图/测试代码告警（factory、text_service、candidate_window、key_filter、file_log）；尚未宣称全仓库 Clippy 通过。共享核心严格 Clippy 单独验证。

## 资源与设备

构建使用已有 Cargo/Gradle/SDK/NDK 缓存，Gradle 使用 `--offline`；没有重新下载构建依赖。当前 `adb devices -l` 无连接设备。Android 真机/模拟器显示、外部宿主和 TalkBack 验收未执行，EM-07/EM-08/EM-12 不可据编译结果完成。

已在线获取本地不存在的 Unicode 17.0、CLDR 48 中文数据及许可，保存在 `expressions/sources` 并记录 SHA-256；后续生成与验证复用这些文件。

## EM-02 Emoji 全集

固定 Unicode 18.0 / CLDR release-49，3972 个输出、1272 个兼容别名，逐序列缺失/额外均为零，全部有中文触发词。相比 17.0 新增 19 个、删除 0 个。`python scripts/build_expressions.py --check` 离线重建一致；`cargo test -p core_engine --test expression_datasets --offline` 独立解析官方数据、遍历全部触发词和别名、导出重载均通过。原始字节校验和、完整许可和覆盖报告已保存。

参考站点标示 3971，官方固定集 3972，总数差 1；站点 HTML 下载 403，未声称站点逐序列等价，详见 `expressions/README.md`。本库以官方全集验收，不因站点计数或本机字形缺失删减。18.0/49 为本次首次获取，17.0/48 复用上次本地文件；生成和测试不再下载。

## EM-03 颜文字合集

固定两个 MIT 数据仓库提交，详细授权范围、来源条款、中文映射核对和排除规则见 `expressions/KAOMOJI-SOURCES.md`。原始 43404 条记录，排除 3375 条（多行/控制字符/HTML 实体残留），合并完全相同的输出后收录 34354 条，547 个不同类别标识全部有中文映射。每个输出具有来源与许可；字体未覆盖不导致替换字符。

`python scripts/build_expressions.py --check` 通过（5 个生成制品）；`cargo test -p core_engine --test expression_datasets --offline` 两项通过，遍历 3972 个 Emoji 和 34354 个颜文字的触发、来源和整库导出重载。数据首次获取后均本地复用。完整词库接入候选、打包、显示和运行时性能仍在后续任务验收。

## EM-04 统一候选追加

`core_engine/tests/expression_candidates.rs` 17 项测试覆盖完整词匹配、子串/递归隔离、跨触发词及两库去重、部分页/满页、数字 1～9、612 项长尾遍历、手机 256 项窗口、Unicode 原样提交、唯一满码暂缓、临时拼音与英文隔离、关闭时旧轨迹、过期 ID、热重载、宿主失败及隐私策略变化。输入候选与模型预测分开；预测候选不追加表情，迟到的模型响应不能改变已发布输入候选及其 ID。

候选序列在查询变化时构造，分页和手机窗口共享缓存；表达式词条只保存词库种类和索引，不按查询复制整库输出字符串。普通文字页后的表情续页可全部翻阅，下一文字组从新页开始。具体优先级和页大小为 10 时的处理见 `docs/expressions.md`。

共享会话覆盖确认前不发布、确定未写入的重试和旧 Windows 主模式清理协议。隐私策略在宿主确认期间改变时，分别更新已发布、待确认与回滚快照，不能暴露未确认文字或在确认后恢复已禁用表情。Windows `reentrant_privacy_change_publishes_only_confirmed_safe_candidates` 验证适配层发布的是确认后的安全视图。

验证结果：

- `cargo test -p core_engine --lib --tests --locked --offline`：246 项通过。
- `cargo test -p im_engine --lib input_session::tests --locked --offline`：25 项通过，包括旧驱动逐键对照、失败恢复与重入宿主范围变更。
- `cargo clippy -p core_engine --all-targets --all-features --locked --offline -- -D warnings`：通过；格式和差异空白检查通过。
- Windows 全目标编译通过；Android `:app:compileDebugKotlin --offline --console=plain` 完成双 ABI 原生构建与 `ExpressionName` 提示解析编译。后续共享会话边界修复由上述核心及 Windows 测试覆盖；最终双端制品仍在 EM-08 集中重建验收。
- Release 使用完整 3972/34354 条本地词库，“开心”得到 858 个候选，遍历结果与两库完整精确索引的并集一致；100 次查询 p50 46 μs、p95 64 μs。此测量不含词库加载、宿主编辑、绘制或 JNI，不代表手机性能。

构建与测试复用本地依赖和词库，无新增下载。未执行本项真实界面交互；字体、视觉及外部宿主验收仍保持未完成，不以这些测试结果替代 EM-06～EM-08。
