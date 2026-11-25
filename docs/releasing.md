# 分平台版本发布

维护者可调用仓库技能 `$mywubi-release`。先从 Windows、Android 各自上次正式交付的提交起草变更说明，选定版本、源码和平台后由用户确认。标签和制品规则见[发布契约](release-contract.md)。

在默认分支部署手动工作流后，推送已确认提交和对应的全局标签。由代理通过 `gh workflow run release.yml --ref <默认分支> -f tag=<标签> -f scope=<windows|android|both> -f request_id=<UUID>` 发起构建。工作流将标签、范围和运行 ID 写入草稿，只构建选中的平台。全部选中任务成功且资产齐全时公开。通过该 UUID 定位准确运行，暂时失败时重跑原运行的失败任务。

首个双端过渡版 [V0.13.0](https://github.com/silevilence/MyWubi/releases/tag/V0.13.0) 包含两端的新版更新检查器。旧 Android 客户端只检查仓库 Latest，旧 Windows 客户端只搜索最近十条。如果提示已是最新版或找不到更新，可按下面步骤手动升级。

- 在[正式发布列表](https://github.com/silevilence/MyWubi/releases)打开双端过渡版本，或之后明确包含本平台的正式版本。展开该版本的 Assets。
- Android 选择原通道的 `MyWubi-<版本>-Android.apk` 或 `-Android-full.apk`，打开 APK，确认系统显示更新已有应用。保留原应用即可继续使用配置、词库和模型。
- Windows 安装版运行该版本的 `MyWubi-win-Setup.exe` 完成覆盖升级。便携版下载 `MyWubi-win-Portable.zip`，退出设置程序后替换程序文件，保留便携目录里的个人配置、码表、词库和模型。

Windows 打包仅包含内置三份码表及许可证。Android 两种 APK 均包含 ARM64 与 x86_64，沿用相同应用签名。每次发布附带范围记录和校验和，Windows 另提供 Velopack 索引及受信组件，Android 另提供两种更新清单与原生库报告。旧版本的标签和组件继续保留。

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
