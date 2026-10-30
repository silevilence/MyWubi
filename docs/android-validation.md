# Android CP-12～CP-17 验收记录

日期：2026-10-30，Asia/Shanghai。此记录区分主机协议、模拟器 JNI、真实外部宿主；
不把两 ABI 编译等同于 ARM64 真机验证。

## CP-12 构建与设备

- Windows 主机：JDK 21.0.8、Rust 1.99.1；安装并验证 aarch64-linux-android / x86_64-linux-android。
- Gradle 8.9 + AGP 8.7.3 + Kotlin/Compose compiler 2.0.21；SDK/Build Tools 35、NDK 27.2.12479018。
- `gradlew :app:assembleDebug :app:assembleDebugAndroidTest` 通过。
- ZIP 内容确认 `lib/arm64-v8a/libmywubi_android.so` 和 `lib/x86_64/libmywubi_android.so`。
  构建任务也断言两个库存在，防止 workspace targetDirectory 配错后得到缺库 APK。
- Pixel 5 / API 34 / x86_64 AVD 启动成功，系统页大小 4096，ADB device 状态正常。
  旧模拟器默认图形后端启动失败；软件渲染启动参数见 android/README.md。
- 没有 ARM64 真机连接；连接条件已记录。真机、16 KB 设备、性能验证留给 CP-22。
- 共享改动后 Windows 工作区测试 277 项通过；工作区构建、核心与 JNI 严格 Clippy 通过。
  保留既有 TSF LNK4104 警告，未替换本机安装的输入法。

SDK 位于 Program Files 下，AGP 对既有 package.xml 有只读提示；固定使用已安装的
Build Tools 35 后构建通过，没有修改权限、文件属性或系统安全策略。

## CP-13 JNI

`NativeContractTest.protocolBoundariesAndLifecycle` 在 API 34 模拟器通过：实际加载
APK 中 x86_64 `.so`，执行建快照/建会话、两阶段确认、候选查询、关闭与重复销毁拒绝。
Rust/C/JNI 读取同一 `core_engine/tests/fixtures/protocol/errors.json` 错误样例；
覆盖版本错误、非法操作、超长窗口、无效句柄、多标量字符拒绝，以及非法 UTF-8。
有效路径验证非 BMP、NUL、组合字符原样传输。JNI 使用当前调用线程的 JNIEnv，
byte[] 复制到 JVM 所有内存，不缓存跨线程环境或借出临时原生指针。

资源接口通过同一个 typed facade 直接开放，Kotlin 没有第二份状态机。
Rust/C 资源版本冲突与 Unicode 词库用例通过；Windows 工作区与 JNI 主机 Clippy 通过。

## CP-14 服务闭环

`ExternalHostTest.shapeInputInSystemSettings` 通过。宿主为系统 Settings 的
`com.google.android.settings.intelligence` 搜索输入框，逐键点击 IME 的 g/g/l/l，
确认宿主预编辑 ggll，点击候选王后正文为王，退格清空。测试先打开并关闭 MyWubi
设置 Activity，随后仍能正常输入。通过 force-stop 后重新启动服务进程，再执行同一
用例通过（27.482 秒），验证资源与会话重新初始化。

初始服务闭环 APK SHA-256：
`95805f88509f9e6c918ea24d5416b8a799de19676ecf90e2273f191638b4e382`。
这是 Compose 接入前制品，后续制品单独回归。自动化以宿主文本达到预期作为断言，
不将 InputConnection 的排队返回当作宿主已经同步完成，也不把搜索框 hint 当作正文。

## CP-15 Compose 键盘

API 34 设备执行 ExternalHostTest 两项与 NativeContractTest，47.087 秒，3 项通过。
系统 Settings 搜索框验证形码上屏、展开选词、大小写布局切换，长按退格删除多个字符且
松手后文本保持稳定。JNI 拒绝旧 query revision 的候选点击。候选正文与编码提示分离，
LazyRow / LazyVerticalGrid 使用稳定候选身份；窗口查询不修改核心排序。
ComposeView 的窗口根节点与视图均绑定 IME 自有生命周期，隐藏/销毁会取消重复按键。
测试对动态标签主动刷新 UiAutomation 缓存，避免旧无障碍节点导致误报。

## CP-16 手机模式与符号

ModeHostTest + NativeContractTest 在设备上 51.483 秒、3 项通过；主机 semantic_events、
semantic_modes、session_contract 共 22 项通过。A03～A06、A16 映射如下：

- 系统 Settings：英文 hello 空格只补一个空格，点击 hello 不补；提交后保留英文。
- 无桌面入口键直接进入拼音；空临时空格/回车无动作，重复进入保留 ni，选你后返回英文。
- 非空英文切主模式先提交原文；数字面板不结束当前轮，未声明数字提交当前内容后字面插入。
- JNI 自定义 charset a1;*：a1/ a; 分别选出甲/乙，a* 返回两候选；不可用拼音拒绝后保留 a。
- 共享核心验证取消返回原模式、失败不提前切换，以及 Windows 一次性英文不加尾空格。

Kotlin 仅发送 SetBaseMode / BeginTemporary / InputText，规则仍由共享核心负责。

## CP-17 输入类型与 Unicode

宿主 Chrome 113.0.5672.136，Android 14 API 34，真实浏览器 InputConnection。
BrowserHostTest 的三条轨迹分别通过，最后 restrictedFieldsAndNextActionInChrome 为
65.073 秒；EditorContractTest 两项通过。A07～A08 证据：

- 非空拼音 Enter 提交 ni，不提交表单；下一次发送才使页面提交计数从 0 到 1。
  空临时模式空格/发送保持 0；搜索也先提交原文、第二次才触发表单；普通正文正常换行。
- 密码输入 ggll 时键盘仅显示安全输入，无候选/原文展示；数字 12、电话 +12 直接输入。
  网址 gg.、邮箱 a@ 使用专用符号键；默认 Next 跳到第二个表单输入框。
- 宿主预置汉字、非 BMP、组合重音、ZWJ emoji、国旗、肤色修饰后，实际点击 IME 退格，
  每次删除完整字素；Ctrl+A 后退格删除选区。预编辑 gg 中左移光标，退格只删光标前 g，
  再输入/删除 a 不覆盖保留的 g。

验收页为 android/tests/host.html，只由本地主机 HTTP 服务提供。Chrome 113 对数字框
无障碍文本附加字段名（12, 数字），测试明确接受这种呈现。显式 enterkeyhint=next 在该
版本只发 Enter；测试默认表单 Next 焦点行为，未在输入法中添加浏览器特判。
依据：Chromium 113 ImeAdapterImpl.performEditorAction：
https://github.com/chromium/chromium/blob/113.0.5672.136/content/public/android/java/src/org/chromium/content/browser/input/ImeAdapterImpl.java#L833

本次 Chrome 首次条款经用户授权接受，可选统计已关闭，不登录账号。

## CP-11～CP-17 整体审核

逐项提交后审核协议边界、JNI 所有权、Compose 生命周期和宿主文本单位，修复四项：

- C 响应缓冲分配使用低两位 0，避免误传资源句柄释放其他响应；跨种类释放回归通过。
- 长按任务监听 IME 生命周期 ON_STOP/ON_DESTROY 立即取消，隐藏键盘后拒绝派发输入。
- InputConnection 的 beginBatchEdit/endBatchEdit 异常纳入结果未知，不崩溃、不重试提交。
- 清空预编辑的 commitText 成功即释放所有权，后续 finish 失败也不把旧范围当作仍然拥有。

最终主机验证：cargo fmt 检查、工作区构建、277 项工作区测试、core_engine 与
mywubi_android 全 targets 严格 Clippy、独立 C 程序编译运行均通过。TSF 仍只有既有
LNK4104 警告；本机已安装的输入法未替换，CP-10 的真实 TSF 记录仍是该平台基线。

Android :app:clean 后 assembleDebug / assembleDebugAndroidTest / lintDebug 通过。
Lint 0 错误、6 警告：备份迁移规则、应用图标、Activity 文字资源化；这些发布完善项
不阻塞本轮输入验收。NDK 两 ABI 均重编译并确认 APK 内库：arm64-v8a 1,565,296 字节，
x86_64 1,789,392 字节。

最终 APK：android/app/build/outputs/apk/debug/app-debug.apk。
SHA-256：ac2ae391adf69d16d11a63a6abc48073be67c3b2fa1e542d451bdf13301dd9de。
ARM64 真机、16 KB 页、最低 API 26 设备及性能未实测；本轮运行证据限于 API 34
x86_64 模拟器中的真实外部宿主。CP-18～CP-22 的后续功能和验收不在本次完成标记内。

测试基建同时修正两类宿主差异：Settings 的动态无障碍文本/候选节点在断言和点击前
刷新缓存；预置长串后用 MoveEnd 放置光标，避免重复点击触发系统拼写建议弹窗。
这些调整不绕过实际输入法按键，失败时仍保留截图和节点树供核验。

最终全量设备运行：`adb shell am instrument -w org.mywubi.test/androidx.test.runner.AndroidJUnitRunner`，
244.221 秒，**OK (12 tests)**。BrowserHostTest 3、EditorContractTest 4、ExternalHostTest 2、
ModeHostTest 1、NativeContractTest 2 全部通过；包含实际按住退格时隐藏键盘后文本不再变化。
主 APK 校验值与上述最终制品一致。
