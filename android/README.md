# Android 开发

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
应用不申请网络权限，输入内容不写日志，不做自动学习。引擎配置位于应用私有
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

从 Windows 迁移码表时，选择“选择码表文件”，通过系统文件选择器同时选择主表及
其直接引用的拼音/英文表，然后明确指定各文件角色。每次最多 32 个文件，每个
不超过 16 MiB；同名文件按选择序号区分。Windows 绝对路径必须重新绑定。
导入文件复制到应用私有目录，完整校验和试建运行时成功后才原子切换活动配置。
取消、校验失败或切换中断不删除旧活动包。旧包暂不自动回收。

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

当前 versionName 为 0.4.0、默认 versionCode 为 2。`install -r` 使用相同签名保留应用数据；卸载再安装不会保留。Debug APK 可直接安装，
默认 Release 输出 `app-release-unsigned.apk`，需签名后安装。发布构建通过环境变量
MYWUBI_KEYSTORE、MYWUBI_STORE_PASSWORD、MYWUBI_KEY_ALIAS、MYWUBI_KEY_PASSWORD
配置签名，四者必须同时提供；密钥不入仓库。`-PversionCode=<递增整数>` 设置升级序号。
替换签名不能直接覆盖旧安装，应先导出数据。APK 内附词表许可证和来源记录。

`.github/workflows/android.yml` 固定工具链，执行核心/JNI 契约、模拟器契约、Lint、
两 ABI 构建、ELF/ZIP 对齐检查并存档制品；不发布商店，不改 Windows Velopack 流程。
设置 UI 的 SAF 测试另需先执行 `adb push android/tests/saf/CP19-main.dict /sdcard/Download/`
和 `adb push android/tests/saf/CP19-pinyin.dict /sdcard/Download/`。
真机测量可传 instrumentation 参数 `-e benchmark_main /data/local/tmp/main.dict`，
运行 DeviceBenchmarkTest；输出应用私有 files/device-benchmark.json。
该数据是 JNI Prepare/Ack 和候选查询耗时，不是键盘渲染或真实宿主上屏延迟。

16 KiB ELF 和 ZIP 静态对齐检查覆盖 APK 中全部原生库；运行兼容性仍需真实设备验证。
当前 ARM64 真机、16 KB 设备、TalkBack 实操及完整联合验收仍有未完成项，详见
[`android-delivery-validation.md`](../docs/android-delivery-validation.md)。
