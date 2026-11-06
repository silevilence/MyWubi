# MI-36～MI-39 实施与验收

日期：2026-11-06。范围为 Windows 组件分发，不改变预测默认关闭或 Android 原生库交付。

## 已实现

- MI-36：应用编译内置的版本化组件清单、固定官方来源、SHA-256、应用/系统/架构兼容性、能力与精确依赖解析。CPU 包为 GGUF/ONNX 共用服务及 CRT；模型包仍是纯数据。
- MI-37：复用有界断点传输，下载前检查空间；独立暂存、压缩包和逐文件校验、同卷原子发布；取消/中断可重试，损坏版本可修复，旧版本保留。目录外占用锁保护正在运行的组件，TSF 只通过 IPC 请求独立服务。
- MI-38：设置页区分组件、模型和运行状态；下载确认展示来源、体积和占用，提供取消/重试；显示旧版本并保护活动组件及共享依赖。准备完成沿用已保存策略，不开启预测或改选模型。保存通过短生命周期辅助进程交接，窗口退出不截断预加载。
- MI-39：统一轻量应用包、独立 CPU 组件 ZIP、编译清单和发布流水线。`build.ps1` 不再把服务及 ONNX Runtime 放入基础包；旧 `-WithModels` 参数只提示统一流程。构建先冻结组件，再编译应用并核对制品哈希。

## 自动与本机验证

组件契约覆盖缺失、不兼容、循环/缺失依赖、Windows 路径别名与损坏。安装与管理测试覆盖取消、损坏更新保留旧版本、同版本修复、空间不足、缓存恢复、未声明 DLL、运行占用及仍被依赖的旧版本移除。

`components_real` 从构建清单固定哈希的完整下载缓存安装正式格式组件，随后清空开发环境变量，仅保留 Windows 系统目录环境，分别通过组件内的独立进程运行 GGUF 与 ONNX。GGUF 的公开前文“北京”返回“市”；ONNX 返回有效候选。配置逐字节保持不变。此项是本机离线缓存恢复与真实原生请求，不代替干净设备在线下载。

设置界面实测证据：

- [确认体积与来源](mi36-39/component-confirm.png)
- [正式地址尚未发布，返回 404](mi36-39/component-source-404.png)
- [相同哈希缓存恢复安装，两后端可用](mi36-39/component-installed.png)
- [移除影响说明，取消保留组件](mi36-39/component-remove-confirm.png)

初始界面证据来自首轮候选制品，随后补充了旧版本清单与目录外占用锁修复；最终制品结果另列，不把旧截图当作最终 DLL 的 TSF 证据。

最终全工作区测试 369 通过、4 个显式设备/真实服务测试忽略（[原始结果](mi36-39/workspace-tests.log)）；其中组件真实测试随后单独在本机和沙盒执行通过。组件与模型模块严格 Clippy 通过。整个 Windows 前端严格 Clippy 仍被原有 COM 原始指针接口、GDI 分块和设置面板告警阻断，不记作通过。

## 不能标为完成的项目

正式组件资产尚未发布，受信 URL 实际返回 HTTP 404；本次只做本地提交，不推送或发布。因此 MI-37/MI-38 的“全新安装后从正式源下载组件及模型”闭环保留未完成。下载失败不自动安装、不改预测设置，基础输入保持独立。

线上 CI 尚未执行。Windows Sandbox CLI 可执行安装与原生请求，但远程图形连接反复显示断开，未取得最终 TSF/GDI 候选显示及上屏证据。以下安装器和便携验证仅覆盖明确列出的范围，不代替可见输入闭环。

## 最终制品及升级证据

最终构建输出为 `target/mi36-39-audited/`，版本 0.7.0、Windows x86_64。`settings.exe` SHA-256 为 `9af34e2452f110e99cd49eaddcaeb2e6367b3c7137dc287c2b8a126bc74eae77`，TSF DLL 为 `92bbf66ea6beca018d9ccc455dcd422e1f2040232f69e217a1f9e0cd80ceeea0`，组件 ZIP 为 `35aa5f327643bbd7b14eb13e554ed0bc0c6fb5c917ff9a4915b22fae30d6eba8`。之后只修改测试适配器、打包校验脚本及文档，不改变这些制品的生产代码。

| 场景 | 实测结果及范围 |
| --- | --- |
| 标准全新安装 | 新建 Windows 沙盒，Setup 静默安装返回 0，安装后程序导出清单成功，程序哈希一致；[日志](mi36-39/fresh-check.txt) |
| 旧基础版升级 | 实际 0.4.0 基础版 Setup → 0.7.0 Setup，均返回 0；合成配置、模型目录标记、用户词库逐字节不变，current 不带 worker/ORT，程序哈希一致；[日志](mi36-39/base-upgrade.txt) |
| 旧模型版升级 | 实际 0.6.2 模型版 Setup → 0.7.0 Setup，同上全部通过；[日志](mi36-39/model-upgrade.txt) |
| 便携全新使用 | 新 ZIP 解压后运行实际 settings 清单命令成功，原生可选载荷不在应用目录；[结果](mi36-39/portable-fresh.json) |
| 便携覆盖升级 | 实际 0.6.2 Portable ZIP → 新 ZIP 覆盖，合成配置、模型目录标记与词库字节保留；旧 worker 文件保留但新加载器不授权它；[结果](mi36-39/portable-upgrade.json) |
| 干净 Windows 原生请求 | 新建沙盒，无开发环境，生产安装器代码从校验缓存安装组件；独立进程清空开发环境变量，GGUF 返回“市”、ONNX 返回有效候选，配置不变；[日志](mi36-39/sandbox-native.log) |
| 本机最终组件 | 同一 ZIP 两后端请求通过；[日志](mi36-39/host-native.log) |

升级使用合成用户数据，模型保留检查为目录标记，并未把真实权重逐字节升级比对当作已完成；真实模型运行使用独立 GGUF/ONNX 样例。安装器测试通过不代表已验证所有旧版本、所有用户目录布局或预测上屏。

## 体积对照

[原始字节数与哈希](mi36-39/sizes.json)。同版本、同架构、同一组当前二进制、同 ZIP deflate 9 参数；“旧模型布局”是重建的布局，不是历史版本发布包实测。MiB 按 1048576 字节换算。

| 布局 | 下载 MiB | 展开 MiB |
| --- | ---: | ---: |
| 基础布局参考 | 16.29 | 33.75 |
| 原模型布局重建 | 26.21 | 61.55 |
| 统一轻量应用 | 16.29 | 33.75 |
| 独立 CPU 组件 | 9.92 | 27.79 |

实际 Velopack Setup 为 24,223,356 字节（23.10 MiB），Portable 为 19,614,910 字节（18.71 MiB）。选择同一 491,400,515 字节模型权重时，旧布局首次启用共 518,882,859 字节，新方式共 518,882,881 字节；拆包只是推迟可选载荷下载，没有消除推理总成本。模型体积为现有 GGUF 示例，不含本地缓存复用。

后续全量更新预算：仅应用 17,079,612 字节；仅组件 10,402,754 字节；未变权重 0。以上是该压缩口径的全量预算，并未测量 Velopack 跨版本 delta；首批精确应用版本规则下，应用升级通常还需相应组件。兼容已安装版本仅在新编译清单显式保留其完整哈希时复用。

## 整体审核与回归

审核补充了安装更新失败后的运行选择：按受信清单顺序选择已安装且兼容的版本，新组件缺失/损坏时仍能使用明确保留的旧版本；不信任旧收据，也不自动启用旧同目录程序。增加回退/不兼容/撤销信任测试，并保护保存模型正在依赖的回退版本。清单重复键按 Windows 大小写语义拒绝。

更新两个显式忽略的真实服务生命周期测试，使其使用实际设置辅助进程和受信组件目录，移除旧“复制 worker 到测试程序旁”假设。适配器编译及相关库回归 95 通过、2 忽略；这些生命周期忽略项未在此轮重新执行，不能当作已验收。脚本测试 6 通过，Rust 格式检查和组件/模型严格 Clippy 通过（[日志](mi36-39/components-clippy.log)）。

可复现命令：

```powershell
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy -p runtime_components -p model_input --all-targets --locked -- -D warnings
python -m unittest discover -s scripts -p 'test_*.py'
./build.ps1 -Version 0.7.0 -OnnxRuntime <1.31.1-x64-dll> -OutputDirectory <新目录>
python scripts/measure_component_distribution.py --application target/velopack-staging --release <新目录> --output <测量目录> --model-bytes 491400515
```

真实组件测试需把 `MYWUBI_COMPONENT_CATALOG` 指向输出清单，`MI_COMPONENT_ARCHIVE` 指向对应 ZIP，`MI_GGUF_PACKAGE`/`MI_ONNX_PACKAGE` 指向真实模型包目录，再运行 `cargo test -p model_input --test components_real --locked -- --ignored --nocapture`。该测试只使用缓存，不声称完成在线下载。整个验证未修改主机已安装输入法或用户配置。
