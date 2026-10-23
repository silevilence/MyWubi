# Android 开发准备度与参考信息

核查日期：2026-10-23（Asia/Shanghai）。代码基线：`faace643cba8a16fb6b1ef4eed9cc313c296304e`。

本文记录现状，不把 ROADMAP 的历史勾选状态作为能力证明。后续设计见 [共享核心与跨端职责方案](cross-platform-core-design.md)，实施与验收见 [实施计划](cross-platform-implementation-plan.md)。这三份文档描述 Android 首版；不包含 iOS。

## 1. 结论

- 已具备启动 Android 开发的核心算法基础；无需另写一套 Kotlin 检索与输入状态机。
- 尚无 Android 工程、InputMethodService、Compose 键盘、完整输入会话 C-ABI 或 JNI 接口，不能视为已经可交付的手机输入法。
- 需要先梳理语义事件、模式生命周期、候选视图、编辑确认、配置与资源接口，再做 Android 输入闭环。
- 本机已有 JDK、Android SDK 和模拟器资源，但 NDK、Rust Android targets 与项目构建链尚未配齐。
- 本轮仅编写文档，没有安装工具、修改系统配置、修改业务代码或替换已安装的 Windows 输入法。

## 2. 当前代码事实

| 范围 | 已有能力 | 跨端缺口与证据 |
| --- | --- | --- |
| 工作区 | core_engine、Windows 输入法、设置程序、TIP 管理器 | [Cargo.toml](../Cargo.toml) 中没有 Android / JNI 项目；[发布流水线](../.github/workflows/release.yml) 为 Windows Velopack |
| 码表 | YAML 头与正文解析、权重、精确/前缀检索、万能键、去重、全拼音节边界 | [dictionary.rs](../core_engine/src/dictionary.rs) 与 [pinyin.rs](../core_engine/src/pinyin.rs) 可复用；不能从桌面基准推断手机延迟 |
| 形码状态机 | 编码、候选、翻页、选词、自动上屏、超长顶字、标点、原文提交 | [state_machine.rs](../core_engine/src/state_machine.rs) 仍混有字符和数字选词语义，页大小限制为 1～10 |
| 临时输入 | 临时拼音/英文、一次提交恢复主模式、主码反查 | [temporary.rs](../core_engine/src/temporary.rs) 的加载依赖入口键；[temporary_input.rs](../core_engine/src/state_machine/temporary_input.rs) 依赖菜单选择并固定一次性生命周期 |
| 中英文 | Windows 全局英文放行实体按键 | [input_session.rs](../windows/im_engine/src/input_session.rs) 的 chinese=false 不运行核心检索；不同于手机持续英文码表补全 |
| 会话协调 | 编辑失败恢复、composition 清理、版本作废、失焦及重载处理 | 目前集中于 Windows [input_session.rs](../windows/im_engine/src/input_session.rs)，混合了共享规则与候选窗口数据 |
| 数据装配 | 系统码表与用户词库合并、运行时快照、启动降级 | 目前在 Windows [lib.rs](../windows/im_engine/src/lib.rs)；加载和合并规则应共享，监听与发布触发分别适配 |
| 用户词库 | CRUD、导入导出、保存失败保留内存状态，已有部分 C-ABI | [user_dictionary.rs](../core_engine/src/user_dictionary.rs) 限制编码为 ASCII 字母数字；并非通用输入会话 ABI，也未实现自动学习 |
| 配置 | TOML 默认值、校验、读写；码表 YAML 引用 | [config.rs](../core_engine/src/config.rs) 混合行为、外观和桌面快捷键；[config_path.rs](../core_engine/src/config_path.rs) 的默认发现策略依赖可执行文件目录与用户配置目录 |
| 设置编辑 | 码表编辑、辅助引用校验、跨目录另存、部分保存失败提示 | [table_session.rs](../windows/settings/src/table_session.rs) 中部分领域规则仍在 Windows 设置层；界面草稿、筛选和对话框不应整体移入核心 |

### 配置字段不等于运行时功能

本次搜索调用链发现：

- `basic.commit_mode` 可序列化、可在设置中选择，但当前输入状态机没有按其分支执行。
- `basic.punctuation_mode` 被传入构造器，却以 `_punctuation_mode` 接收；实际以当前状态机分支为准。
- `dictionary.enable_exact_match` 没有接入当前会话检索参数；会话构造检索选项时固定 `prefer_exact: true`。
- `dictionary.enable_fuzzy` 属于预留字段，没有模糊音输入能力。
- `hotkey.toggle_simplify` 没有实际快捷键处理调用链。

跨端重构必须保留这些旧字段的读取和往返能力，不能以“修好配置”为由偷偷改变 Windows 行为。若要启用其语义，应作为单独行为变更另行设计与验收。Android 首版也不得展示为已经生效的功能。

### Windows 行为事实

- 主模式中英文切换会提交原始编码；临时模式切换则取消本轮输入。
- 失焦/成功重载时，主模式结束 composition 并保留宿主已有预编辑文本；临时模式删除本轮预编辑文本。
- 临时模式仅在 `InputSession::handle` 存在 rollback 快照、编辑失败且确认未提交时保留输入重试；切换/失焦/重载并不创建相同 rollback。已经写入文字但清理失败时，不重放提交。
- 主模式已有不同的失败恢复策略，不能在接口抽取时统一改成临时模式策略。
- 临时菜单和模式标签不会进入宿主文档。反查只使用主码表文件，不包含仅存在于合并用户词库中的编码。
- `set_chinese` 在执行宿主编辑之前更新开关，`reload` 在清理前采用新状态机/config/revision；清理失败不会把开关或快照恢复成旧值。候选视图的发布时点与这些处理状态不同。
- `OnTestKeyDown` 的外层准备会进行上下文清理、运行时同步、Shift 追踪与恢复；仅核心 `test_key/would_consume` 是纯查询。现有 `RequestEditSession` 使用 `TF_ES_SYNC | TF_ES_READWRITE`。
- 配置读取、保存、活动辅助表加载具有不同的校验范围；入口关闭时运行时不加载辅助表，设置保存非空引用时仍校验。路径层还保留 TSF 不重定向到 AppContainer 私有目录的 Windows 处理。

依据为 [输入会话及其测试](../windows/im_engine/src/input_session.rs)、[临时输入说明](temporary-input.md) 和 [2026-10-22 验收记录](temporary-input-validation.md)。后者是历史实测，不代表本次重新完成了真实 TSF 验收。

以上时序与校验差异在 [第二轮方案审核](cross-platform-plan-review.md) 中进一步核对，并已回写设计及验收矩阵。

## 3. 本机开发工具

| 工具 | 2026-10-23 实查结果 | 后续动作 |
| --- | --- | --- |
| Rust / Cargo | 1.99.1；现有 Windows 核心测试可运行 | 锁定项目采用的工具链并记录 |
| Rust targets | 仅 `x86_64-pc-windows-msvc` | 补 `aarch64-linux-android`、`x86_64-linux-android` |
| JDK | `C:/Program Files/Android/openjdk/jdk-21.0.8`；java/javac 21.0.8 均可运行；另有 JDK 8 | 依据最终 Gradle/AGP 兼容组合选用，不能只凭已安装就判定兼容 |
| Android SDK | `C:/Program Files (x86)/Android/android-sdk`；platforms android-34、android-35 | 按选定 compileSdk/targetSdk 补包并固定版本 |
| Build Tools | 35.0.0 | 随构建版本矩阵验证 |
| SDK Manager | 通过 latest/bin/sdkmanager.bat 运行，报告 19.0；本地清单可读取 | 配置项目开发入口；本次未接受许可证或安装包 |
| NDK | 当前 SDK 安装清单无 NDK，也未配置 NDK 环境变量 | 安装并固定版本，验证 Rust 链接产物 |
| ADB | PATH 中为 `C:/Softs/platform-tools/adb.exe`，35.0.2；SDK 内另有 34.0.5 | 构建调试脚本统一指定 SDK 内工具，避免混用版本 |
| 设备 | `adb devices -l` 返回空列表 | 真机验收需连接 ARM64 Android 设备 |
| 模拟器 | Emulator 34.1.19；Pixel 5 / API 34 / x86_64 AVD 和镜像均存在 | 实际启动、安装和输入链路尚未验证 |
| 模拟器加速 | 探测退出成功并报告 WHPX 可用，同时有旧工具的 Hyper-V 提示 | 不能据此宣称模拟器已可用；启动实测后再判断，不据这条混合输出修改虚拟化配置 |
| Android Studio | 常用位置和安装记录未发现 | 可选开发 IDE；并非命令行构建的硬前提 |
| Gradle / Wrapper | 当前 PATH 未发现 Gradle；仓库无 Gradle Wrapper | 新工程提交固定版本 Wrapper；不要求全局 Gradle 常驻 |
| 命令环境 | 当前终端无 JAVA_HOME、ANDROID_HOME/NDK 等相关环境变量 | 项目级配置或开发入口显式定位；本次仅在子进程临时指定 JDK 验证工具 |
| cargo-ndk | 当前 cargo 子命令中未发现 | 不是既定构建方案的强制依赖；先按项目约定验证 rust-android-gradle |

检查方式包括 `Get-Command`、常用安装目录与卸载注册表、`rustup target list --installed`、`java -version`、`javac -version`、`sdkmanager --list_installed`、`adb devices -l`、`emulator -list-avds` 和加速探测。未做全盘文件搜索；“未发现”不等同于证明任意自定义目录下均不存在。

## 4. 验证记录与限制

- 本次讨论实测 `cargo test -p core_engine --locked --offline`：138 项通过，0 失败。
- 本次讨论实测 `cargo clippy -p core_engine --all-targets --locked --offline -- -D warnings`：通过。
- 本轮补充实测 `cargo test --workspace --quiet --locked --offline`：233 项通过（核心 138、Windows 输入法 68、设置 21、TIP 管理器 6），0 失败；仍有既有 LNK4104 导出警告。历史 2026-10-22 的 230 项结果不得作为当前数量使用。
- 未进行 Android 交叉编译、APK 构建、设备运行、JNI 生命周期测试、手机内存/耗电测试。
- 既有性能记录使用桌面主机与合成数据；其索引内存不是 Android 进程 PSS，也不代表真实手机端时延。

## 5. 外部技术参考

以下官方资料核对于 2026-10-23。它们用于确定平台适配要求，不替代项目自己的兼容测试。

- [Android 输入法开发](https://developer.android.com/develop/ui/views/touch-and-input/creating-input-method)：服务、输入类型、候选及组合文本。
- [InputConnection](https://developer.android.com/reference/android/view/inputmethod/InputConnection)：预编辑、提交、删除与编辑器交互。
- [JNI tips](https://developer.android.com/ndk/guides/jni-tips)：JNI 线程、引用与字符串转换。
- [Storage Access Framework](https://developer.android.com/training/data-storage/shared/documents-files)：由系统选择器取得文件，URI 不等于普通文件路径。
- [NDK 安装与版本配置](https://developer.android.com/studio/projects/install-ndk)、[Android 构建的 JDK 选择](https://developer.android.com/build/jdks)、[Gradle Wrapper](https://docs.gradle.org/current/userguide/gradle_wrapper.html)。
- [16 KB 页大小兼容](https://developer.android.com/guide/practices/page-sizes)：原生库的 ELF/打包对齐及设备验证要求。
- [rust-android-gradle 上游](https://github.com/mozilla/rust-android-gradle)：沿用项目约定，具体插件版本和任务接线需用当前工具组合验证。
