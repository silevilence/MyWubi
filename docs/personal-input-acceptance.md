# MI-40～MI-52 开发与集中验收记录

日期：2026-11-12。Windows x64 本机、Windows 11 Sandbox 26100、Android API 34 x86_64 模拟器。
用户确认暂没有授权个人输入数据和 Android 真机；因此使用可人工核对的合成事件，保留真实效果和真机验收。
按用户要求，完整功能验证集中在实现后执行；后续重复只针对本轮发现的问题或修改。
全部依赖、SDK、码表、运行库和程序复用本地缓存；Cargo/Gradle 使用 offline，没有下载新模型或验证资源。

## 执行结果

| 验证 | 结果与范围 |
| --- | --- |
| Rust 工作区 | `cargo test --offline --workspace --no-fail-fast`：447 通过、0 失败、6 忽略 |
| 审核后定向回归 | core / personal / im_engine / Android native：364 通过、1 忽略；退出与 Windows 修改后：92 通过、1 忽略；最终个人数据用例 13/13 通过 |
| 静态检查 | workspace/all-targets 编译通过；core_engine / personal_input / mywubi_android 严格 Clippy 通过；完整严格 Clippy 被原有 im_engine COM 指针签名和 settings 样式告警阻挡，未做无关清理 |
| Windows | settings.exe、im_engine.dll 离线 Release 构建；Sandbox Edge 本地编辑框和复用本机经典记事本实测 |
| Android | ARM64 与 x86_64 原生库、Debug APK、测试 APK 及 JVM 测试离线构建通过；ARM64 未在真机执行 |
| Android 集中仪器测试 | 最终 14/14 通过：PersonalDataTest 2、PersonalHostTest 1、PredictionHostTest 1、NativeContractTest 2、LifecycleContractTest 6、SettingsModelTest 2 |
| 双向交换 | Windows 的 100 条事件明文/加密包在 Android 导入、训练、再导出；返回 Windows 均 0 新增 / 100 重复 / 0 冲突。Android 原生生成的 42 条事件可导入 Windows |
| 退出清理 | 本轮 Sandbox 和新启动的无窗口模拟器均已关闭；未注册或替换宿主电脑当前输入法 |

## Windows 宿主对照

Edge 与经典记事本均通过 `w → 空格` 上屏“明天去”，随后展示个人模型候选“图书馆”，再次空格得到“明天去图书馆”。
事件中个人候选具有独立来源、请求身份和实际模型版本，提交为 Confirmed。Edge 撤销后正文恢复“明天去”且候选消失。
敏感字段的 `w` / Escape 探针未产生采集事件；普通形码候选仍可能由基础输入路径显示，这不等于开启个人采集或个人预测。

经典记事本在开头插入“实验室”时，事件保留右文“明天去图书馆”；全选替换时保留选区“实验室明天去图书馆”，
最后只提交“图书馆”。其系统字体缺少中文显示字形，界面为方框，但候选、UTF-16 位置及采集文本一致。
验收环境原本未安装记事本，本次复用本机既有二进制及 MUI，没有联网下载。

不能据此宣称完整捕获所有宿主操作：Edge 本轮撤销没有产生可与预测提交关联的编辑事件。
不可观察通知不能被当作已证明的负反馈；受限/AppContainer 宿主、其它浏览器和完整连续更新/回退交互矩阵保留验收。
现有实现只在可靠观察到回退至提交前左右文时关联删除；其它外部操作使用 Unknown。

## 数据、训练与隐私回归

13 个个人数据测试覆盖：明文/加密往返、错误密码与破坏版本、稳定身份循环去重、同身份冲突、
预览后新输入保护、8 个并发写者、截断日志恢复、保留期限/墓碑、被淘汰的编辑引用、未知/失败/纠正样本排除、
初训/增量重训/重复数据/回退/覆盖失效/取消、三个开关八种组合、敏感门控和撤销待写队列、
正常退出刷写，以及有记录但没有有效前文时不发布伪就绪模型。

Android 外部宿主验证了真实 InputConnection 上屏，关闭收集后仍能使用已有个人模型且记录数量不增加。
测试恢复原配置和个人目录；日志只有测试结果与状态，不输出个人正文或密码。合成交换包只在本机及本地模拟器间传递。
初次界面测试被模拟器 System UI ANR 弹窗阻挡；旧生命周期测试依赖外部残留码表，已改为自足的小型 fixture。
新的个人宿主用例改为等待配置应用及异步正文结果，不再用固定短睡眠断言。最终合并的一轮 14 项全部通过。

## 30 分钟合成成本测试

`personal_check OUTPUT_DIR 1800` 的核心输入回放与后台追加/训练同时运行，实际完成 82,897 轮。
每轮准备/确认一个编码和一次空格提交；预测使用预载的合成模型对象。每 30 秒追加 20 条合成事件并触发有界训练。
这项测试不含 TSF/Android 渲染、真实 InputConnection 往返，也不测每次预测的模型文件加载成本。

| 指标 | 记录 |
| --- | --- |
| 输入两步处理 P50 / P95 / P99 | 35 / 75 / 271 μs |
| 预测成功返回预期合成词 | 82,897 / 82,897；不是个人质量分数 |
| 初训耗时 | 19 ms |
| 结束时模型文件 | 50,674 字节 |
| 资源监测覆盖 | 1,758 秒、351 个样本（监测器比回放启动晚） |
| 监测到的峰值工作集 | 14,630,912 字节，约 14 MiB |
| 监测末次进程累计 CPU | 30.890625 秒；不是整机 CPU 百分比或功耗 |

该长测在最终审核修正前启动。后续修正涉及正常退出刷写、空模型门控、事件元数据与状态展示，已做定向回归；
没有重复占用桌面跑另一轮 30 分钟。小型合成集不代表 128 MiB 数据上限下的内存、移动端温控或长期习惯变化。

## 任务结论与保留项

已验收并原地勾选：MI-40、MI-44、MI-45、MI-46、MI-47、MI-49。
其它条目的已实现子项已勾选，整体保持未完成：

- MI-41：许可、异步队列、并发与恢复已验证；真实磁盘耗尽、共享便携目录权限隔离及压力上限未专项验收。
- MI-42：Edge / 记事本完成基本采集与编辑对照；受限宿主及部分撤销/外部变化的观察范围仍不完整。
- MI-43：模拟器闭环完成；缺 Android 真机，宿主短读/旋转/连接重建完整设备矩阵保留。
- MI-48：只有两个统计基线的合成 80/20 对照；未作通用/神经模型对照、独立保留测试集与真实个人质量选型。
- MI-50：双端预算调度和状态入口已实现；本地核心长测完成，真实宿主长时延迟和真机热/电量未验收。
- MI-51：双端个人候选上屏和关闭收集后继续预测已通过；完整更新/连续选择/纠正/受限宿主矩阵仍保留。
- MI-52：合成跨端交换和功能回归已通过；没有个人数据净收益及真机证据，不宣称“持续变聪明”。

## 可复查证据

仓库测试：`personal_input/tests/personal_flow.rs`、`core_engine/tests/prediction_session.rs`、
`android/app/src/androidTest/java/org/mywubi/PersonalDataTest.kt` 与 `PersonalHostTest.kt`。

本机忽略目录 `target/` 保存完整日志与合成证据：
`mi40-52-final-tests.log`、`mi40-52-last-regression.log`、`mi40-52-shutdown-regression.log`、
`mi40-52-final-personal-tests.log`、`mi40-52-final-core-clippy.log`、`mi40-52-android-acceptance.log`、
`mi40-52-windows-complete-build.log`、`mi40-52-android-complete-build.log`、
`mi40-52-soak/report.json`、`mi40-52-soak-resources.json`、
`mi40-52-windows/events-observed.json`、双向 `.mwp` 包与 `mi40-52-android-host.png`。
这些运行产物未纳入 Git；没有推送远端。
