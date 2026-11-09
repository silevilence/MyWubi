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
