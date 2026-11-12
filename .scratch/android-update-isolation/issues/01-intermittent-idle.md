# 更新下载隔离测试偶发停留在 Idle

Status: needs-triage

## 现象与证据

- 2026-11-09，Android validation 运行 42572922054（提交 2a41d46，attempt 2）执行 29 项模拟器测试，28 项通过；`UpdateIsolationTest.slowCheckAndDownloadPreserveEveryEditAndCandidateOrder` 断言期望 `Downloading`，实际 `Idle`。
- 日志：https://github.com/silevilence/MyWubi/actions/runs/42572922054/attempts/2
- 同一提交运行 42572921970 成功；V0.8.0 最终提交 34a4f32 的运行 42573978412 也成功，包括 Device protocol and resource contracts。不能据此认定偶发问题已修复。
- 最终提交验证：https://github.com/silevilence/MyWubi/actions/runs/42573978412
- 当前未确认是产品初始化竞态还是测试时序问题，尚无稳定复现。后续应在复用已有 Android 环境的前提下，单独复现此测试并记录 `UpdateDownload` 初始化与 start 的状态变化。

## 验收

- 建立能够重现原始 Idle/Downloading 不一致的最小测试。
- 确认根因后修复，并验证初始化、开始、取消及恢复下载状态。
- 不以简单重复整套流水线直到通过代替问题定位。
