# 双平台版本发布

1. 修改 Cargo.toml 的 workspace.package.version，以及 android/app/build.gradle 的
   versionName；Android versionCode 必须递增。用 Cargo 更新本项目 lockfile 版本，
   在 CHANGELOG.md 顶部添加唯一的 `## Vx.y.z` 更新说明。
2. 提交并推送分支，在 GitHub Actions 手动运行 **Release**。该路径运行同样的
   Windows/Android 构建与检查，输出 artifacts，但不会创建 Release。
3. 预检成功后，在该提交创建并推送 `Vx.y.z` 标签。标签、Cargo 与 Android 版本不符
   会被拒绝。Windows 测试/Velopack 打包、Android 核心测试/Clippy/Release Lint/
   签名与双 ABI 对齐检查全部通过后，先上传至草稿 Release，再公开发布。

发布资产包括 Windows Setup.exe、Portable.zip、Velopack nupkg/更新索引，以及
`MyWubi-x.y.z-Android.apk`、原生库检查 JSON 和 `SHA256SUMS.txt`。APK 包含 ARM64
与 x86_64 两种架构，可直接安装；不要求用户自行从 AAB 转换。
Windows 打包仅包含内置三份码表及许可证，不收集本地自定义/未跟踪词表。

`.github/workflows/android.yml` 继续承担日常 Android 模拟器契约测试。
发布工作流通过不代表 ARM64 真机、16 KB 设备、TalkBack 或最终 TSF 手工验收已完成；
实际覆盖边界见 [交付记录](android-delivery-validation.md)。

## Android 签名

仓库 Actions Secrets 必须存在：

| 名称 | 内容 |
| --- | --- |
| MYWUBI_KEYSTORE_BASE64 | 项目专用发布 keystore 的 Base64 |
| MYWUBI_STORE_PASSWORD | keystore 密码 |
| MYWUBI_KEY_ALIAS | 密钥别名 |
| MYWUBI_KEY_PASSWORD | 私钥密码 |

工作流仅将密钥解码到临时 runner 目录，不上传到 artifacts，不存入 Gradle/Rust 缓存。
Secret 值不放在仓库、命令参数或发布说明中。缺失时构建失败，不生成开发签名替代品。

首次公开 APK 从 V0.5.0 开始使用专用发布签名。以后必须沿用这份密钥以支持覆盖升级；
之前本地 debug key 签名的测试 APK 需先导出词库，再卸载并安装公开版。

2026-10-31 配置时，本机备份位于 `%USERPROFILE%\.android\mywubi-release.jks`，
配套凭据记录为 `%USERPROFILE%\.android\mywubi-release-signing.json`。
记录中的 password_dpapi 使用 Windows DPAPI 加密，只有原 Windows 用户环境可解密。
迁移电脑或重装前，应在该账户下安全导出密码到密码管理器，并备份 keystore；仅复制
DPAPI 记录到另一台电脑不能恢复密码。GitHub 不提供已有 Secret 明文回读。

官方参考：[Android 应用签名](https://developer.android.com/studio/publish/app-signing)、
[GitHub CLI Secret 管理](https://cli.github.com/manual/gh_secret_set)。
