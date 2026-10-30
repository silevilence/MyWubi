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
