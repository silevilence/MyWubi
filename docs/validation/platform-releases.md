# REL-01～07 分平台发布验收 · 2026-11-25

## 本地矩阵

| 场景 | 执行与结果 |
| --- | --- |
| 双端 → Windows → Android → 双端 | Python 发布契约测试得到两端最新版本 1/1、2/1、2/3、4/4。重复标签、倒退、错误范围、Android versionCode 重用及首次直接单端发布均被拒绝。 |
| 选择、取消、失败与恢复 | 三种范围分别枚举两端 success、skipped、failure、cancelled 共 48 种结果。正常选中组合才能公开。模拟部分上传中断后原运行补齐缺件，同名不同摘要、不同运行和已公开版本均被拒绝。正式双端运行结果见下方。 |
| 资产与组件 | 把本地 V0.7.3 Windows 包、受信 CPU 组件和本地签名的 V0.12.1 Android 标准／完整包硬链接到各自的验收输入，分别经过生产 assemble，每端得到 8 个文件。发布端校验暴露了中文说明在 Windows 默认编码下读取失败，改为 UTF-8 后复测通过。篡改组件、另一端混入增量包、缺失清单及缺失平台记录均有失败样例。 |
| 发布技能 | 独立离线演练以 Windows V2、Android V3 为基线准备 V4，覆盖共享改动、单端积压、用户改选、仅草稿、补传和必需资产缺失。元数据核验测试限制可访问的 API 端点，安装包、组件和模型均无需回下载。 |
| 客户端发现 | 两端均覆盖跨过 11 条其他平台记录、按标签绑定下载和损坏清单。最终复核增加“草稿创建顺序和版本顺序不同”的回归，按完整分页记录的数字版本排序。Android 同时覆盖最新版本不兼容时回退及分页中断，Windows 便携入口保留原标签大小写。 |

## 本次实际升级

**Windows。** 在断网的 Windows Sandbox 内静默安装缓存的 0.7.3。测试程序通过沙盒回环 HTTP 提供受控发布列表和本地打包的 0.12.1 更新源。检查后列表不再允许访问，Velopack 仍下载选定 nupkg、校验并覆盖安装。安装后的 sq.version 为 0.12.1，settings.exe 与打包输入摘要一致。逐字节比对 AppData 下的配置、用户词库及 ONNX 模型通过。初次启动无已登录用户，第二次在无桌面系统会话使用原生命令执行。沙盒脚本执行策略保持原设置。沙盒已关闭。

| Windows 证据 | SHA-256 |
| --- | --- |
| 旧版 Setup | `7fb095734a7100d9a30cd778c59b481b706e23f93ce044ff7adeabc6e111d078` |
| 验收用 0.12.1 完整 nupkg | `26a2f2e3731448612e32626952765c3710746ed1a7968bec8aafc82423655bcc7` |
| 覆盖安装后的 settings.exe | `3ed84db498e59834ab11d4d77cf1fa824a5c81e776a25c3b096c75fa1a0977dc` |

**Android。** API 34 x86_64 无窗口模拟器使用已有只读镜像。AppUpdateTest 7 项、UpdateDownloadTest 4 项通过，后者使用同签名新旧 APK、异包名及异签名 APK 校验。标准包从 versionCode 20 经应用更新下载器与系统安装器升至 21。独立的安装后测试确认设备上的 APK 摘要和更新缓存，逐项核对配置、导入码表、用户词库及放入测试数据目录的 ONNX 模型。设备上的更新包摘要为 `2d5c6dd74fbc20565e9d2368308b2a5be20a61fd72e564cf4c72287e07c27078`。本次升级使用本地测试签名。模拟器已关闭，连接的 ARM64 真机未参与。完整通道覆盖升级及旧包迁移沿用[前次实测记录](mi53-57.md)。

实际安装完成后增加的乱序发布回归在宿主机运行。便携入口检查覆盖目标链接生成，未在桌面点击浏览器。Windows 本次走完整包，未制作专项增量包。Windows 组件兼容测试、Android 固定 ONNX 组件 JVM 测试独立执行。

## 复核入口

- `python -m unittest discover -s scripts -p 'test_*.py' -v` 覆盖发布契约、资产、草稿和公开后检查。`cargo fmt --all -- --check` 和 `cargo test --workspace --locked --offline` 覆盖 Rust。并行构建引发的 im_engine 文档测试 E0463 已单独用 `cargo test --workspace --doc --locked --offline -j 2` 复测通过。最终串行工作区运行 621 项通过、8 项忽略；其中隔离安装专项已单独在沙盒执行。最终版本排序及便携链接定向回归 4 项通过。
- Android 离线执行 `:app:assembleDebug :app:compileDebugAndroidTestKotlin :app:testDebugUnitTest :app:lintDebug`。JVM 单测 21 项，Lint 为 0 错误、10 警告。设备端另执行安装后数据核验。
- 本机证据在 `target/rel-qa/`，包括 `android.log`、`output/fixture.log`、`output/pass.json`、前后版本记录、安装文件摘要及两端的 `assemble/` 输出。构建和测试日志在 `target/rel-*.log`。本轮复用缓存和本地 APK、安装包、组件、模型及镜像，仅对 GitHub 正式发布元数据和 Android 小型更新清单进行在线查询。

## 真实发布

两端的上次正式版本均为 V0.12.1（提交 `4cc3399`，Android versionCode 20）。为使旧客户端能先升级到支持分平台检查的版本，推荐首次双端发布。用户确认以 `1f021e5` 为源码基础发布 V0.13.0。发布准备提交 `6dfbbb4413cee1aa17b0a828244e28f35eba1b86` 仅同步版本、锁文件及说明。

2026-11-25 14:03（北京时间），GitHub 公开双端正式版。标签、两端交付记录均指向上述发布提交，Android versionCode 为 21。四个任务全部成功，CI 留有元数据、Windows 和 Android 三份上传记录。[正式 Release](https://github.com/silevilence/MyWubi/releases/tag/V0.13.0) · [Actions 运行 45556246552](https://github.com/silevilence/MyWubi/actions/runs/45556246552)

`scripts/verify_release.py` 已核对发布说明、范围、标签提交、运行身份、资产上传状态，以及构建清单与平台元数据的大小和摘要。共 14 项资产，缺失 0 项。旧客户端可按[发布维护](../releasing.md)覆盖升级。

| 已公开资产 | 字节 |
| --- | ---: |
| `MyWubi-0.13.0-Android-full-native.json` | 2,657 |
| `MyWubi-0.13.0-Android-full.apk` | 49,914,172 |
| `MyWubi-0.13.0-Android-native.json` | 1,442 |
| `MyWubi-0.13.0-Android.apk` | 21,911,916 |
| `MyWubi-0.13.0-full.nupkg` | 40,240,645 |
| `MyWubi-components-cpu-0.13.0-windows-x86_64.zip` | 10,454,012 |
| `MyWubi-win-Portable.zip` | 40,167,164 |
| `MyWubi-win-Setup.exe` | 44,774,917 |
| `SHA256SUMS.txt` | 1,209 |
| `android-update-full.json` | 917 |
| `android-update.json` | 912 |
| `components.json` | 5,219 |
| `release-scope-both.json` | 2,279 |
| `releases.win.json` | 1,516 |
