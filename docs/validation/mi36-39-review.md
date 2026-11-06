# MI-36～MI-39 实施与验收

日期：2026-11-06。范围为 Windows 组件分发，不改变预测默认关闭或 Android 原生库交付。

最新进展：用户授权推送和发布后，V0.7.1 的正式发布与 Windows/Android CI 均已成功；Windows Sandbox 的显示连接现已恢复。首轮的 HTTP 404 和未发布限制属于历史记录，不再是当前阻塞。当前尚未完成的是干净沙盒在线下载组件及模型后的完整预测上屏闭环，详见文末。

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

## 首轮未完成项目（发布前）

正式组件资产尚未发布，受信 URL 实际返回 HTTP 404；本次只做本地提交，不推送或发布。因此 MI-37/MI-38 的“全新安装后从正式源下载组件及模型”闭环保留未完成。下载失败不自动安装、不改预测设置，基础输入保持独立。

线上 CI 尚未执行。Windows Sandbox CLI 可执行安装与原生请求，但远程图形连接反复显示断开，未取得最终 TSF/GDI 候选显示及上屏证据。以下安装器和便携验证仅覆盖明确列出的范围，不代替可见输入闭环。

## 首轮审计制品及升级证据

首轮审计构建输出为 `target/mi36-39-audited/`，版本 0.7.0、Windows x86_64。`settings.exe` SHA-256 为 `9af34e2452f110e99cd49eaddcaeb2e6367b3c7137dc287c2b8a126bc74eae77`，TSF DLL 为 `92bbf66ea6beca018d9ccc455dcd422e1f2040232f69e217a1f9e0cd80ceeea0`，组件 ZIP 为 `35aa5f327643bbd7b14eb13e554ed0bc0c6fb5c917ff9a4915b22fae30d6eba8`。后续补验发现并修复了客户端崩溃恢复问题，最终修订制品及验证范围见本文件末尾。

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

更新两个显式忽略的真实服务生命周期测试，使其使用实际设置辅助进程和受信组件目录，移除旧“复制 worker 到测试程序旁”假设。适配器编译及相关库回归 95 通过、2 忽略；这两个生命周期项在后续干净沙盒补验，见下文；普通测试命令仍默认忽略真实服务操作。脚本测试 6 通过，Rust 格式检查和组件/模型严格 Clippy 通过（[日志](mi36-39/components-clippy.log)）。

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

## 后续生命周期补验与恢复修复（2026-11-06）

正式组件地址再次请求仍为 HTTP 404；GitHub API 对本地提交 `b211e828847d1d584a5af8af5c30124b6d2adc36` 返回 Actions 运行数 0。本次仍未推送或发布。

在同一干净 Windows 沙盒中，使用发布设置程序、编译清单及固定哈希组件执行两个真实服务测试：

- 设置辅助进程退出后约 4.71 秒 Ready，首个普通请求约 164 毫秒返回“市”；无请求空闲 61 秒后仍驻留，关闭预测变 Idle，再启用复用同一 PID。[日志](mi36-39/sandbox-handoff.log)
- 崩溃恢复首次失败，两次可复现。组件哈希校验约 2.2 秒耗尽候选请求的 1 秒预算，服务已启动但尚未收到 Configure；客户端直接返回 Timeout，使服务无法准备模型。[原始失败](mi36-39/sandbox-client-before-fix.log)、[定位日志](mi36-39/sandbox-client-diagnosis.log)
- 修复把后台服务启动等待独立限制为 5 秒，保留原请求截止时间，并在发送候选请求前拒绝过期请求。恢复测试改用 50 毫秒预算断言原请求 Timeout，服务仍进入 Ready；显式恢复正常预算并完成配置后，新请求返回“市”，重新激活复用同一 PID。测试通过，耗时约 10.92 秒。[回归日志](mi36-39/sandbox-client-regression.log)

本次相关库回归 95 通过、2 个真实服务测试默认忽略；随后上述两项分别显式运行通过。临时诊断代码已移除，格式和 diff 检查通过。这些测试调用实际 IPC 客户端和独立服务，不是 TSF/GDI 可见上屏证据。

最终修订安装包位于 `target/mi36-39-recovery/`，原生组件 ZIP 完全不变。Setup 为 24,223,369 字节，Portable 为 19,614,923 字节；[修订制品哈希](mi36-39/recovery-artifacts.json)。修订安装包在沙盒中全新静默安装、运行清单命令和安装后 DLL 哈希比对通过（[日志](mi36-39/recovery-installed.txt)）。前述旧版升级和体积布局对照属于首轮审计制品；本修订仅变更 IPC 恢复逻辑，未重复旧版升级场景，也不将其记成最终修订包的重复验收。

## 正式发布与沙盒桌面恢复（2026-11-06）

用户随后授权推送并发布组件。为保留已存在的 V0.7.0，发布修订版本 [V0.7.1](https://github.com/silevilence/MyWubi/releases/tag/V0.7.1)，标签指向 `5dccebb46ff20446a158b0b12dae85f7fb89e381`。[Actions 42161329247](https://github.com/silevilence/MyWubi/actions/runs/42161329247) 的 Windows、Android 和发布任务全部成功。正式 CPU ZIP 为 10,338,068 字节，SHA-256 为 `794b61325ac095c706eeaa820364b571ad692204d6d41bbbb18a1a97ce155402`；Setup 为 24,222,738 字节，Portable 为 19,614,756 字节。下载的 Setup 与 Portable 已对照正式 SHA256SUMS 校验。

主机独立测试目录中的正式设置程序已从 GitHub 下载并安装 CPU 组件，显示 GGUF/ONNX 均可用；预测配置仍为关闭。新增显式在线测试从空目录调用生产下载和安装代码：本机完成组件安装，但随后模型下载超时；沙盒的组件请求也发生 `timeout: receive response`。这些失败不记为在线闭环通过。部分下载保留在不可激活的缓存目录，设置程序仍可操作。

沙盒显示故障与下载超时分别定位。原沙盒已登录，但 System 事件 10121/10120/10111 报告 `RdpIdd.dll` 的 UMDF 驱动失败，随后远程会话以原因 17 断开。仅禁用 vGPU 未解决。修复步骤仅作用于临时沙盒：

1. 用 `<Configuration><VGpu>Disable</VGpu><MemoryInMB>4096</MemoryInMB></Configuration>` 启动沙盒。
2. 通过 `wsb exec --run-as System` 在**沙盒内**设置 `HKLM\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services\fEnableWddmDriver` 为 DWORD `0`，原值不存在。
3. 在**沙盒内**执行 `shutdown.exe /r /t 0`，等待其重启，再连接并关闭失效的重连提示。没有重启主机或修改主机显示策略。

该设置对应微软的 [RDP WDDM 显示驱动策略](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-terminalserver#ts_server_wddm_graphics_driver)，要求重启生效；[Windows Sandbox 支持内部重启保留状态](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-overview)。在本机组合操作后恢复桌面；未单独验证策略变更和重启各自的必要性，不将其宣称为所有同类错误的通用修复。

复查时沙盒重启后已运行 6 分钟，驱动崩溃事件为 0（[结果](mi36-39/sandbox-repair-check.json)）。资源管理器、正式设置程序和记事本均可交互。正式 0.7.1 Setup 全新安装返回 0；仅在沙盒内注册输入法，记事本选中 MyWubi 后，默认码表 `a` 的“工”候选显示并可用空格上屏。组件下载失败期间基础输入仍能使用。这项仅证明基础 TSF/GDI 输入，不代表模型预测已上屏。用户主机未注册测试输入法，现有 AppData 配置未修改。

MI-37～MI-39 父任务仍保持未完成：继续补验正式源在线下载、模型预测上屏及其离线/重启/后端切换流程。发布与沙盒桌面故障已经解除，不再把二者列为阻塞。

## 可见预测、离线重启与下载超时修复

正式 0.7.1 在恢复后的沙盒补验：

- 设置页从正式 SHA-256 缓存恢复 CPU 组件安装，随后显式选择 GGUF 模型、设置提前加载和 1000 ms 等待、开启预测并保存。测试码表只包含 `a → 工` 和 `n → 北京`，替换范围仅为沙盒内测试配置目录；模型包使用已校验的本地样例。这不是全新在线下载闭环。
- 记事本输入“北京”后显示 GGUF 候选“市、区、县”，空格选词后实际保存为“北京市”。[候选截图](mi36-39/mi371-gguf-visible.png)、[提交截图](mi36-39/mi371-gguf-committed.png)、[保存文本](mi36-39/mi371-gguf-committed.txt)。独立服务路径为沙盒 AppData 下的 `components/cpu--0.7.1/model-worker.exe`。
- 设置页确认移除正在使用的 CPU 组件时，拒绝删除并要求先切回词典预测保存。[保护提示](mi36-39/mi371-active-removal-protected.png)。
- 设置页切换 ONNX 样例并保存，显示候选并选词后保存为“北京天气”。[候选截图](mi36-39/mi371-onnx-visible.png)、[提交截图](mi36-39/mi371-onnx-committed.png)、[保存文本](mi36-39/mi371-onnx-committed.txt)、[受控目录 ORT 加载路径](mi36-39/mi371-onnx-loaded.json)。该样例只验证原生评分、准入和显示链路，其宽松阈值不代表模型质量达标。
- 沙盒内部重启后，停用其唯一虚拟网卡，在未启动设置程序的情况下重新选择 MyWubi，保存的 ONNX 模型仍显示“天气、大学、时间、奥运、现代”并可选词。[离线网卡状态](mi36-39/mi371-offline-network.json)、[重启后离线候选](mi36-39/mi371-offline-after-restart.png)。验证结束恢复网卡；主机网卡未操作。
- 同一沙盒使用生产下载器从 HF 镜像空目录下载模型，前两次失败后从保留偏移继续，第三次完成全部 491,411,858 字节并通过校验发布。[完整续传记录](mi36-39/mi371-model-online-resume.log)。模型下载通过不替代 GitHub 组件下载通过，也不把此前导入模型的截图算作这次在线模型的完整 UI 闭环。

另发现可复现的下载缺陷：`ureq 3.3.0` 在正文阶段继续应用已经完成的响应头超时，慢速但持续传输也会报 `timeout: receive response`；[上游修复说明](https://github.com/algesten/ureq/pull/1194)。本地 TCP 测试立即发完响应头、发送一个字节后延迟正文，100 ms 响应头预算和 2 秒正文预算下，旧依赖在约 0.32 秒内失败（[修复前](mi36-39/download-timeout-before.log)）。

更新至 `ureq 3.5.1` 后该测试通过；生产配置为连接 10 秒、响应头 20 秒、单次读写停滞 20 秒、正文总预算 10 分钟。新增停滞超时后保留部分字节并恢复下载测试，下载相关 9 项全部通过（[修复后](mi36-39/download-timeout-after.log)）。修复后的本机测试从空组件目录经正式 GitHub URL 安装并运行 GGUF/ONNX，配置保持关闭（[日志](mi36-39/fixed-host-component-online.log)）。沙盒同一在线测试仍有网络失败，不能据本机成功代替干净 Windows 验收。

本次修复的工作区回归 371 通过、6 个显式真实环境测试默认忽略（[日志](mi36-39/download-fix-workspace.log)），组件与模型严格 Clippy 通过（[日志](mi36-39/download-fix-clippy.log)）。版本准备为 0.7.2；上述可见预测属于已发布的 0.7.1，后续 0.7.2 正式发布和制品验收另行记录。MI-37～MI-39 父任务继续保留未完成。
