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
