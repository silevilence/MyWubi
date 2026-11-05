# MI-27～MI-31 整体验收

2026-11-05 按任务分别实现、验收并本地提交；仅勾选原有任务条目。配置与下载不隐式启用预测，模型试运行独立于活动输入；CPU 预算仍限定为 50～1000 ms。模型仍标为实验状态，没有以功能成功替代质量推荐。

| 任务 | 本地提交 | 验收说明 |
| --- | --- | --- |
| MI-27 | `6535768` | [双端配置与实际运行状态](model-input-settings.md) |
| MI-28 | `6a45532` | [HF 与镜像下载、取消与续传](model-input-downloads.md) |
| MI-29 | `98696ee` | [ModelScope 独立映射与三源同一性](model-input-downloads.md) |
| MI-30 | `91c3b25` | [双端真实下载至输入框上屏](model-input-download-ui.md) |
| MI-31 | `5c33db2` | [本地包接入、错误诊断与双端试运行](model-input-custom.md) |

整体代码检查覆盖配置默认值和保存、下载固定身份及失败恢复、包校验与移除边界、JNI/工作器隔离、试运行不改变活动配置、界面缺组件提示。修复了下载后的重复权重占用、保存按钮可达性、只读操作误触发草稿确认，以及试运行进度动画与 CPU 推理的资源竞争。

工作区 `cargo test --workspace` 的 [47 组结果](validation/mi27-31-workspace-tests.log)为 355 通过、0 失败、3 忽略；需要真实模型的本地导入/试运行另行执行并[通过](validation/mi31-real-local-idle.log)。`cargo clippy --workspace --all-targets` [通过](validation/mi27-31-clippy.log)，保留 6 条既有 settings 风格警告。最终 `cargo fmt --all -- --check` 与 Git 空白检查通过。

整体验证中将调度测试的交付期限直接推进到过期状态，消除了依赖 80 ms 线程启动时序的假失败；生产调度器与预测预算不变。上述工作区回归包含此修正。随后仅调整 Android 界面与设备测试，最终 APK 重新构建并执行 5 项设备验收：[下载到真实上屏 1 项](validation/mi30-android-network.log)、[兼容性/试运行/设置回归 4 项](validation/mi31-android-tests.log)全部通过。[最终 APK 哈希](validation/mi30-31-android-artifacts.json)用于复测。

Windows 在 Sandbox 中完成真实设置与记事本操作；Android 实测为 API 34 x86_64 模拟器，arm64 JNI 构建通过但不代表 ARM 真机性能。Android 最终下载验收第一次提交没有预测候选，第二次新提交成功，逐次结果和首次失败日志保留。试运行推理耗时 Windows 810 ms、Android 798 ms；这不构成首次命中率或整体质量认证。MI-32 之后的 GPU 工作不在本次范围内。
