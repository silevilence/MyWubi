# Android 开发

最低 Android 8.0 / API 26，编译/目标 API 35。固定 Gradle 8.9（Wrapper 校验 SHA-256）、
AGP 8.7.3、Kotlin/Compose compiler 2.0.21、Compose BOM 2024.09.03、
Build Tools 35.0.0、NDK 27.2.12479018、rust-android-gradle 0.9.6。
本机验证 JDK 21.0.8；构建检查 Rust 1.99.1，避免工具链静默漂移。Java/Kotlin 字节码目标 17。
rust-android-gradle 的链接包装器需要 PATH 中有 Python 3。

先安装 JDK 21、Android SDK platform 35 / Build Tools 35.0.0 / platform-tools /
NDK 27.2.12479018，并按 SDK Manager 要求接受许可证。安装 Rust target：

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
./android/build.ps1 -Sdk '<SDK目录>' -Jdk '<JDK目录>'
```

非 Windows 环境设置 JAVA_HOME / ANDROID_HOME 后，`cd android && ./gradlew :app:assembleDebug`。
rust-android-gradle 自动编译两 ABI 并将 `.so` 放入 APK；不需要每键建引擎或手工复制库。
输出 `app/build/outputs/apk/debug/app-debug.apk`。根 Cargo workspace 仍可在 Windows
执行 `cargo test --workspace --locked --offline`（首次获取依赖后）。

设备测试：连接已授权 USB 调试的 ARM64 Android 8+ 真机，或启动 x86_64 AVD；
`adb devices -l` 应显示 device，不是 unauthorized/offline。然后：

```powershell
./android/build.ps1 -DeviceTests
```

本机 Pixel 5 / API 34 AVD 的默认图形后端启动失败，软件渲染可启动：

```powershell
& '<SDK目录>/emulator/emulator.exe' -avd pixel_5_-_api_34 -no-window -no-audio -no-snapshot -gpu swiftshader_indirect -feature -Vulkan
```

使用该 SDK 的 platform-tools/adb，避免 PATH 中多个 adb 混用。没有连接 ARM64 真机时，
两 ABI 编译成功不代表真机运行已通过；性能和 16 KB 设备验证属于 CP-22。

当前内嵌五笔 86 是仓库开发示例表，界面明确提示；拼音/英文资源与许可来源见
`../tables/README.md`、`../tables/licenses/`。完整表导入与编辑属于 CP-19/20。
应用不申请网络权限，输入内容不写日志，不做自动学习。引擎配置位于应用私有
files/engine/config.toml；首次启动只补缺失文件，后续启动不覆盖用户文件。

协议见 `../docs/native-protocol.md`。JNI 使用有界 byte[] 传输标准 UTF-8，避免
Modified UTF-8 与 UTF-16 混淆；每次输出由 JVM 独立持有，不借出原生内存。

构建依据：[AGP 8.7](https://developer.android.com/build/releases/past-releases/agp-8-7-0-release-notes)、
[rust-android-gradle 0.9.6](https://github.com/mozilla/rust-android-gradle/tree/v0.9.6)、
[JNI](https://developer.android.com/ndk/guides/jni-tips)。具体验收见 `../docs/android-validation.md`。
