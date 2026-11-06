# Android 开发

V0.7.0 的字词预测、模型管理、下载与试运行、预加载均为**实验性功能**，预测默认关闭。标准发布 APK 不内置模型运行库或权重，本地模型需按[双端接入说明](../docs/model-input-integration.md)构建模型版；当前没有通过质量验收的推荐模型，ARM64 真机的耗电、温度与持续运行仍待验收。

最低 Android 8.0 / API 26，编译/目标 API 35。固定 Gradle 8.9（Wrapper 校验 SHA-256）、
AGP 8.7.3、Kotlin/Compose compiler 2.0.21、Compose BOM 2024.09.03、
Build Tools 35.0.0、NDK 27.2.12479018、rust-android-gradle 0.9.6。
本机验证 JDK 21.0.8；构建检查 Rust 1.99.1，避免工具链静默漂移。Java/Kotlin 字节码目标 17。
rust-android-gradle 的链接包装器需要 PATH 中有 Python 3。

先安装 JDK 21、Android SDK platform 35 / Build Tools 35.0.0 / platform-tools /
NDK 27.2.12479018，并按 SDK Manager 要求接受许可证。安装 Rust target：

```powershell
rustup toolchain install 1.99.1 --profile minimal
rustup override set 1.99.1
rustup target add --toolchain 1.99.1 aarch64-linux-android x86_64-linux-android
./android/build.ps1 -Sdk '<SDK目录>' -Jdk '<JDK目录>'
```

非 Windows 环境设置 JAVA_HOME / ANDROID_HOME 后，`cd android && ./gradlew :app:assembleDebug`。
rust-android-gradle 自动编译两 ABI 并将 `.so` 放入 APK；不需要每键建引擎或手工复制库。
输出 `app/build/outputs/apk/debug/app-debug.apk`。根 Cargo workspace 仍可在 Windows
执行 `cargo test --workspace --locked --offline`（首次获取依赖后）。

设备测试：连接已授权 USB 调试的 ARM64 Android 8+ 真机，或启动 x86_64 AVD；
`adb devices -l` 应显示 device，不是 unauthorized/offline。然后：

```powershell
# 在单独终端启动本仓库的浏览器验收页（主机仅监听回环地址）
python -m http.server 8765 --bind 127.0.0.1 --directory android/tests
# 测试终端，使用同一 Android SDK 的 adb
adb reverse tcp:8765 tcp:8765
adb push android/tests/saf/CP19-main.dict /sdcard/Download/
adb push android/tests/saf/CP19-pinyin.dict /sdcard/Download/
./android/build.ps1 -DeviceTests
```

外部宿主 UI 测试使用英文系统界面的 Settings、DocumentsUI 和 Chrome；首次运行 Chrome 时由使用者完成条款页。
测试仅访问设备映射的本地验收页，不登录账号。测试会启用并选择 MyWubi 输入法，并在
Settings 搜索框及 Chrome 表单输入测试文字。Gradle connectedDebugAndroidTest 完成后会卸载
测试安装的应用；需保留 APK 时手动 `adb install -r` 两个 APK 后执行 `adb shell am instrument -w
org.mywubi.test/androidx.test.runner.AndroidJUnitRunner`。当前已验证的宿主版本见验收记录。

本机 Pixel 5 / API 34 AVD 的默认图形后端启动失败，软件渲染可启动：

```powershell
& '<SDK目录>/emulator/emulator.exe' -avd pixel_5_-_api_34 -no-window -no-audio -no-snapshot -gpu swiftshader_indirect -feature -Vulkan
```

使用该 SDK 的 platform-tools/adb，避免 PATH 中多个 adb 混用。没有连接 ARM64 真机时，
两 ABI 编译成功不代表真机运行已通过；性能和 16 KB 设备验证属于 CP-22。

当前内嵌五笔 86 是仓库开发示例表，界面明确提示；拼音/英文资源与许可来源见
`../tables/README.md`、`../tables/licenses/`。安装后在设置中启用并选择 MyWubi。
应用使用网络权限检查和下载应用更新，不发送输入内容；输入内容不写日志，不做自动学习。引擎配置位于应用私有
files/engine/config.toml；首次启动只补缺失文件，后续启动不覆盖用户文件。

持续中英模式与一次性拼音使用共享核心规则；数字符号按钮只切布局。
EditorInfo 决定密码/数字/电话/网址/邮箱的直接输入与回车动作。非空回车先提交本轮，
再次回车才执行发送/搜索；空临时模式不会发送。密码不显示预编辑与候选。
无预编辑时退格按 ICU 字素边界删除，已有宿主选区优先删除；光标移出预编辑后保留
宿主正文并放弃旧范围，不用旧长度回删。Android 使用 UTF-16 范围，JNI 传输标准 UTF-8。

协议见 `../docs/native-protocol.md`。JNI 使用有界 byte[] 传输标准 UTF-8，避免
Modified UTF-8 与 UTF-16 混淆；每次输出由 JVM 独立持有，不借出原生内存。

构建依据：[AGP 8.7](https://developer.android.com/build/releases/past-releases/agp-8-7-0-release-notes)、
[rust-android-gradle 0.9.6](https://github.com/mozilla/rust-android-gradle/tree/v0.9.6)、
[JNI](https://developer.android.com/ndk/guides/jni-tips)。具体验收见 `../docs/android-validation.md`。

## 设置、导入和维护

输入设置提供唯一候选自动上屏、溢出顶字、编码提示、用户词库开关，保存到共享
config.toml。振动和本地预编辑兼容选项单独存储，后者下次进入输入框生效。
“已保存”与“运行时已应用”分别显示；加载失败保留旧引擎并显示错误。
退出未保存页面会提示保留草稿，重新打开可恢复；磁盘冲突不会覆盖原文件。

手机上的码表加载步骤：

1. 将 MyWubi UTF-8 文本码表放入手机的“下载”目录（或其它系统文件选择器可访问的位置）。
2. 打开 MyWubi，点击首页“码表”区域的“从手机导入码表”，在系统文件选择器中选择文件。
3. 单个文件会自动选为主码表，点击“校验并启用码表包”。看到“运行时已应用”后即可使用；输入法未启动时，下次打开键盘会加载。
4. 如果主表引用了拼音或英文辅助表，长按多选主表和所需辅助表，再分别指定角色。同名文件不会自动匹配。

文件内容须符合 MyWubi 文本码表格式：每行为 `编码<Tab>词条[<Tab>权重]`，可包含 `---` 包围的码表头；示例见 `../tables/wubi86.dict`。
不直接支持搜狗 `.scel`、RIME `.yaml` 等格式，需先转换为 MyWubi 格式。
文件会复制到应用私有目录，不需要授予整个存储空间的访问权限。
从 Windows 迁移时，同样通过以上入口选择主表与直接引用的辅助表。每次最多 32 个文件，每个
不超过 16 MiB；同名文件按选择序号区分。Windows 绝对路径必须重新绑定。
导入文件复制到应用私有目录，完整校验和试建运行时成功后才原子切换活动配置。
取消、校验失败或切换中断不删除旧活动包。旧包暂不自动回收。
选择文件后可用“取消本次导入”退出导入流程，已修改的设置草稿仍可保存。

“编辑码表与用户词库”可编辑主表/辅助表类型、字符集、码长、引用及词条，或维护
用户词库词条和权重。保存调用共享 Rust 校验，失败保留草稿；用户词库导入按
code+word 合并，并支持系统文件选择器导出。这里是手动维护，没有自动学习或拼音学习。
用户词库应先导出备份，再卸载或清除应用数据；应用关闭系统备份。

## 发布制品与设备检查

```powershell
./android/gradlew.bat -p android :app:assembleRelease :app:lintDebug
python android/check_apk.py android/app/build/outputs/apk/debug/app-debug.apk --output target/android-native.json
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

当前 versionName 为 0.6.0、默认 versionCode 为 5。`install -r` 使用相同签名保留应用数据；卸载再安装不会保留。Debug APK 可直接安装，
默认 Release 输出 `app-release-unsigned.apk`，需签名后安装。发布构建通过环境变量
MYWUBI_KEYSTORE、MYWUBI_STORE_PASSWORD、MYWUBI_KEY_ALIAS、MYWUBI_KEY_PASSWORD
配置签名，四者必须同时提供；密钥不入仓库。`-PversionCode=<递增整数>` 设置升级序号。
替换签名不能直接覆盖旧安装，应先导出数据。APK 内附词表许可证和来源记录。

## 应用内更新

键盘打开时在后台检查 GitHub 正式版本，成功后 24 小时内不重复检查；失败从 15 分钟
开始退避。检查不读取输入内容，不下载 APK，也不会弹窗、发通知或索取安装权限。
只有预编辑、候选和临时会话均已结束，且停止触摸/按键至少 1.8 秒后，设置按钮才显示
静态更新提示点；英文直输、数字和密码连续输入时同样隐藏。设置按钮可打开现有设置页。

在设置页选择“应用更新”，可查看当前版本、主动检查和阅读更新说明。检查失败可立即
重试，不会误报“最新”。点击“下载更新”后显示进度，可取消或失败重试；目前不自动下载。
下载保存于应用私有目录，进程被终止时未完成部分会被舍弃，再打开设置后可重新下载；
完整的安装包会重新校验后恢复为待安装。该任务不依赖输入法服务存活。

点击“安装更新”前先结束当前输入。应用复核 SHA-256、包名、递增版本、系统/ABI 和签名，
再按需打开系统“允许来自此来源的应用”。授权返回后需再次点击安装，最后由系统确认。
拒绝授权、取消安装或系统安装失败时保留已校验文件，处理原因后可重试。安装会重启
输入法、影响当前会话；同签名覆盖安装保留已保存的配置、导入码表和用户词库。
不能通过卸载再安装来替代覆盖升级，卸载会删除这些数据。

旧版首次升级需要手动下载并安装带此功能的 APK；此前发布没有 `android-update.json`
时不能完成应用内检查。新发布流程从签名 APK 生成更新信息并随 Release 一起公开，
每次发布仍需递增 `android/app/build.gradle` 的 versionCode。Debug 安装只能覆盖同一
调试签名的测试包，不能直接升级为正式签名包。

更新专项验收（只操作测试模拟器；默认保留基础版本，添加开关才实际覆盖升级）：

```powershell
./android/test-updates.ps1 -Serial emulator-5554
./android/test-updates.ps1 -Serial emulator-5554 -InstallUpgrade
```

脚本构建基础/递增版本与错误签名 APK，运行取消、重试、校验与系统授权/取消测试。
如果模拟器已有更高版本，用 `-BaseVersionCode <已安装序号>`，不要降级或卸载来绕过。
覆盖安装测试会在模拟器导入验收码表，并逐文件比较升级前后的引擎数据 SHA-256。
实际安装会终止原应用的测试进程，脚本随后单独运行升级后验证。
设备范围、性能数据及尚未验证的系统行为见 [更新验收记录](../docs/android-update-validation.md)。

`.github/workflows/android.yml` 固定工具链，执行核心/JNI 契约、模拟器契约、Lint、
两 ABI 构建、ELF/ZIP 对齐检查并存档制品；不发布商店，不改 Windows Velopack 流程。
设置 UI 的 SAF 测试另需先执行 `adb push android/tests/saf/CP19-main.dict /sdcard/Download/`
和 `adb push android/tests/saf/CP19-pinyin.dict /sdcard/Download/`，
另推送 `android/tests/saf/Standalone-main.dict` 到同一目录验证单文件导入。
真机测量可传 instrumentation 参数 `-e benchmark_main /data/local/tmp/main.dict`，
运行 DeviceBenchmarkTest；输出应用私有 files/device-benchmark.json。
该数据是 JNI Prepare/Ack 和候选查询耗时，不是键盘渲染或真实宿主上屏延迟。

16 KiB ELF 和 ZIP 静态对齐检查覆盖 APK 中全部原生库；运行兼容性仍需真实设备验证。
当前 ARM64 真机、16 KB 设备、TalkBack 实操及完整联合验收仍有未完成项，详见
[`android-delivery-validation.md`](../docs/android-delivery-validation.md)。

## GitHub 双平台发布

推送 `V0.6.0` 形式的版本标签后，Release 工作流同时构建 Windows Velopack 包和
包含 arm64-v8a / x86_64 的通用 Android APK。两平台通过检查后才公开同一个 Release，
APK 名为 `MyWubi-<版本>-Android.apk`，附带原生库检查报告和 SHA256SUMS.txt。
手动运行 Release 工作流只做构建预检，不发布版本。

Android 签名必须配置 MYWUBI_KEYSTORE_BASE64、MYWUBI_STORE_PASSWORD、MYWUBI_KEY_ALIAS、
MYWUBI_KEY_PASSWORD 四个仓库 Actions Secrets；缺少任意值即失败，不回退到开发签名。
签名密钥需在后续版本继续使用，不能每次构建重新生成。
V0.5.0 是首个公开体验 APK；它与此前本地开发签名 APK 不兼容覆盖安装，迁移前先导出词库。
维护与签名备份说明见 [发布维护](../docs/releasing.md)。

## 键盘布局与图标

字母页采用等宽、错位的三行 QWERTY；大小写在第三行左侧，退格在右侧，长按退格连续删除。
底栏提供数字、符号、标点、宽空格、中英切换和宿主回车动作。数字页采用独立三列数字盘；
电话输入保留 `+ * #`，网址和邮箱输入在字母底栏提供 `/` 或 `@`。展开候选时用候选网格替换字母区。
符号页可切换中英文符号；英文、密码、网址及邮箱场景默认半角符号。展开候选后底栏保留退格。
左上角 MyWubi 图标打开工具栏，可进入“设置 / 码表”或切换系统输入法。支持系统深浅色。

布局参考：[搜狗](https://shouji.sogou.com/)、[百度](https://srf.baidu.com/)、
[讯飞](https://srf.xunfei.cn/)的手机键盘，以及 [Trime 社区简洁布局](https://github.com/osfans/trime/issues/304)。
采用常见的按键位置和分区，不包含这些输入法的皮肤、语音或预测功能。

图标源文件在 `../assets/branding/mywubi.png`，导出脚本生成 Android 各密度与自适应图标、
Windows 多尺寸 ICO 及设置页 PNG。修改源图后运行 `../assets/branding/export-icons.ps1`。
