# CP-18～CP-24 实施与验收记录

日期：2026-10-31，Asia/Shanghai。任务状态以 ROADMAP 为准。

## CP-18：会话生命周期

API 34 / x86_64 / 4096 字节页模拟器。LifecycleContractTest 6 项和
EditorContractTest 4 项通过；BrowserHostTest 原有 3 项通过，新增生命周期轨迹
hideRotateAndSwitchEditorsDoNotReuseOldCandidates 单独重跑通过。

- 主线程 FIFO 保留重入输入次序；有界队列溢出明确报告，连接变化清除旧代次动作。
  控制事件无需等待当前编辑确认，每个宿主步骤前后再次检查 token 与捕获的连接。
- View 隐藏保留会话；重显可继续 gg 输入。同一连接的重启保留状态；无法证明连接
  连续性时作废代次。旧提交后的第二步骤不能写入新编辑器。
- false、异常、批编辑边界失败均不当作“确认未写入”；放弃无法重证的预编辑所有权，
  Ack 后不重放提交、不按旧长度删除新选区。Recover 接口已接入普通输入前。
- JNI 验证本地预编辑、Busy、未知结果后可继续、换上下文后旧 Ack 失效；坏资源
  重建不破坏当前可用会话。加载序号拒绝迟到结果及销毁后结果。
- Chrome 113 真实宿主验证隐藏重显、横屏恢复、切换两个输入框、跨应用返回，正文
  不串写；测试进程结束及下一次 instrumentation 启动也重新建立服务和 native 句柄。
- 横屏候选/模式区与按键区分列，避免纵向键盘挤满宿主。Chrome 旋转会关闭并重建
  编辑连接，重新点入字段后可输入；点击旧拼写会弹系统拼写建议，测试先关闭该菜单。
  字段被键盘遮挡时先隐藏键盘再点击，并确认新字段获得焦点后才按键。

真实宿主与代理 InputConnection 故障注入是两类证据，不互相替代。
没有在用户主机注册或替换 Windows 输入法。

## CP-19：设置与资源包

ResourceContractTest 3 项通过：引用缺失/越界和版本冲突保持原配置；切换前后故障
注入均能重新 BuildRuntime；首次准备只补缺失文件；TouchSave 不被未使用的桌面
热键冲突阻止，也不写入手机 UI 字段。EngineStore 在活动配置替换前试建完整运行时，
旧包不自动回收。通知失败单独提示，不把已落盘结果误报成保存失败。

SettingsUiTest 两项在 API 34 模拟器通过：旋转、退出恢复草稿、SAF 取消、重新载入
放弃草稿，以及通过系统 DocumentsUI 多选主/辅助表、显式指定角色、重绑定 Windows
绝对引用后启用。测试结束恢复原配置。SAF 样本在 android/tests/saf/。
后台 BuildRuntime 不再持有全局句柄锁，避免构建期间阻塞活动会话调用。

## CP-20：码表与用户词库编辑

LibraryContractTest 在 JNI 上验证非 BMP 词条增删改、无效编码拒绝、同 code+word
导入合并、权重更新、导出和外部修改冲突。LibraryModelTest 在真实 Android 生命周期
组件上验证非法元数据保存保持原文件、未保存草稿经 ViewModel 销毁重建恢复、正常
词条落盘，以及外部修改后保存失败仍保留草稿。两项通过。

主/辅助码表编辑均使用共享 ReadTable/SaveTable，用户词库使用共享 EditUser/ExportUser；
没有另写 Android 校验或合并算法。切换读取失败不会把旧表单绑定到新文件。
界面明确标注手工维护，不声称自动学习或拼音学习。导入后提示重新载入，避免把旧
列表视为新数据。原始数据与草稿分别保存，退出、重新载入和删除均有明确提示。
最终 APK 另通过设置入口打开 LibraryActivity，检查主表类型/字符集/码长等元数据
和用户词库表单、导入导出入口及非空词库计数，页面正常渲染；未以截图代替上述数据契约测试。

## CP-21：交互检查（未完成）

ExternalHostTest 的候选展开/选取、大小写切换、连删释放停止、隐藏键盘停止连删、
短按 ACTION_CANCEL 不删除已在 API 34 真实 Settings 搜索框验证。浏览器横竖屏和
键盘重建、设置草稿旋转恢复也通过。按钮提供单独标签、禁用状态和可访问点击动作。

本次测得 g 键可访问节点为 42.91 × 48 dp：高度达到 48 dp，窄屏字母键宽度仍未达到
48 dp 目标，尚需触摸目标设计复核。模拟器未安装 TalkBack，未声称读屏实操通过；
振动开关已实现，但没有真机验证触感。因此 CP-21 两条综合验收仍保留未完成。

## CP-22：设备测量与对齐（未完成）

最终代码 APK 0.4.0 / versionCode 2，在 sdk_gphone64_x86_64 / API 34 / 4096 字节页
运行 DeviceBenchmarkTest。真实主表 123,517 条，辅表为内嵌拼音/英文；每模式 1,000
轮，宽前缀分别 a/n/a，每轮读取前 256 个候选。完整样本哈希、APK 哈希和结果见
[原始 JSON](android-benchmark-emulator.json)。主表仅作本地测量，没有提交或分发。

| 模式 / 操作 | p50 µs | p95 µs | p99 µs |
| --- | ---: | ---: | ---: |
| 主表 Prepare + Ack | 892.2 | 3008.0 | 5727.6 |
| 主表候选查询 | 2884.7 | 8491.7 | 14438.1 |
| 拼音 Prepare + Ack | 318.7 | 1227.8 | 2163.1 |
| 拼音候选查询 | 1501.7 | 4217.4 | 9763.2 |
| 英文 Prepare + Ack | 492.4 | 1773.3 | 3335.2 |
| 英文候选查询 | 1571.2 | 4714.9 | 8955.6 |

本轮初次 BuildRuntime 334.04 ms；文件此前已解析，不能称为冷缓存/冷进程加载。
测量开始、三个模式结束、释放后的 PSS 为 390562 / 406017 / 407161 / 398085 /
343909 KiB，包含 instrumentation、Java 词条 JSON 与 Android UI 等内存，不能作为
独立引擎稳态内存。19.55 秒测试不能替代长时间资源增长或真实宿主上屏延迟。

APK 全部 4 个原生库（含 AndroidX 库）通过两 ABI ELF LOAD 及 ZIP 16 KiB 对齐；
zipalign -c -P 16 通过。mywubi 两 ABI 均导出 version/exchange JNI 符号，NEEDED
仅 libc.so / libdl.so。模拟器列出 arm64-v8a 翻译支持不等于 ARM64 实机运行。
缺少 ARM64 真机、16 KB 运行环境、冷启动/端到端/长时间测试及设备目标阈值验收，
因此 CP-22 两条综合任务均继续未完成。

## CP-23：构建、签名与升级

固定 Rust 1.99.1、JDK 21、Gradle 8.9、AGP 8.7.3、SDK/Build Tools 35、NDK
27.2.12479018。`.github/workflows/android.yml` 定义核心/JNI 测试、严格 Clippy、
Lint、两 ABI 构建和 API 34 模拟器契约测试，存档 APK、报告及词表许可。
本地对应步骤通过；本次只本地提交，未在 GitHub 触发工作流。Windows release.yml
没有改动。商店分发和正式密钥发布不在此次完成范围。

- Debug、未签名 Release 构建成功；Lint 为 0 errors / 2 warnings（应用图标与
  Android 12+ dataExtractionRules 配置提示，allowBackup=false 已设置）。
- 签名环境变量分支使用本机开发 debug key 试构建成功；正式密钥未读取、未入库。
  Debug APK 的 v2 签名经 apksigner 验证。未签名 Release 不能直接安装。
- 0.4.0 / versionCode 1 → 2 以 adb install -r 升级，7 个活动配置、默认表、导入包
  文件的 SHA-256 全部不变。另以包含非 BMP 词条的非空用户词库验证覆盖安装，
  原字节保持不变。签名相同才支持直接升级；卸载/清数据不承诺保留。
- APK 包含词表许可证、README 与 sources.json；开发示例主表未替换成未授权词表。

## 最终构建与联合验收边界

主机 cargo test --workspace --locked --offline 共 277 项通过；核心和 Android native
严格 Clippy（-D warnings）、cargo fmt、git diff --check 通过。重建 core_engine DLL
后重新编译执行 abi_smoke.c，通过。workspace Release 构建通过。

最终 versionCode 2 整套设备回归 26 项，413.26 秒，25 通过 / 1 失败：SAF 导入后
下拉按钮点击未打开菜单。补充滚动后的布局稳定等待和精确菜单标签匹配后，针对该项
重跑 34.46 秒通过（target/cp24-saf-recheck.log）。26 个功能用例均有通过记录，但
修正后未再次重跑整套，不把最后一次整套结果写成全绿。另独立性能用例通过。

| 制品 | SHA-256 |
| --- | --- |
| Android Debug 0.4.0 / code 2 | d48e33bb00f7a6e51438188646eafa30cb90ef48de70cd737b928429cf41a253 |
| Android Release unsigned / code 2 | 823e977d57fa696c520114d019085f0ec4f9b46475435b8c7d6669902030b781 |
| 最终 im_engine.dll | 2d8bc07d10d31938aa00fff42b4fbc1bedbbca7e36fc55722a6785985ecd3cd1 |
| 最终 settings.exe | f343e2da8ae1955b147857f7bbc83bdf1a99af733ae6f38bf1ebd5e84dfc29e9 |

Debug APK 13,338,387 字节，未签名 Release 10,885,681 字节；本地输出在
android/app/build/outputs/apk/。最终 Windows 文件来自本次 release 构建，不采用旧
CP-10 DLL。但隔离 Sandbox 的 setup.ps1 被来宾系统脚本执行策略拒绝
（PSSecurityException / UnauthorizedAccess），没有修改策略或绕过限制。
已停止本次创建的沙盒；未在用户主机注册输入法。最终真实 TSF 验收未执行，CP-24
第一条继续未完成；文档和矩阵已更新，不能把自动测试替代为真实 TSF 通过。

回归过程中保留了失败记录：Chrome 一次焦点窗口 ANR 连带阻塞 UI 用例；随后每个
浏览器场景改为独立启动。Settings 测试补充 Activity 启动等待、只构造一次 selector、
清除上一用例的 Activity 任务；SAF 用资源 ID 区分同名的工具栏与侧栏 Downloads。
这些是测试隔离/节点定位修正，不通过忽略异常或删掉断言隐藏失败。
本地日志在 target/cp24-*.log；真实设备仍缺 ARM64 / 16 KB / TalkBack 覆盖。

## CP-24：逐项证据索引（未完成）

下表区分自动化证据与真实宿主验收。W 系列全部仍缺本次最终 DLL 的真实 TSF 重验，
不能沿用 CP-10 早期 DLL 的通过结论。测试名称对应仓库测试或 src 内单元测试。
A/R/B 的通过范围限本次 x86_64 模拟器及主机契约，不代表所有设备或故障机制。

| 编号 | 场景 | 证据与限制 | 当前边界 |
| --- | --- | --- | --- |
| W01 | 普通形码、大小写、自定义 charset | state_machine_flow / semantic_events | 自动化已回归；最终 TSF 待验 |
| W02 | 精确/前缀、重码权重、去重、万能键 | candidate_contract / reverse_lookup | 自动化已回归；最终 TSF 待验 |
| W03 | 自动上屏、最大码长、溢出顶字 | state_machine_flow / semantic_events | 自动化已回归；最终 TSF 待验 |
| W04 | 空格、回车、退格、Esc、空缓冲 | state_machine_flow / session_contract | 自动化已回归；最终 TSF 待验 |
| W05 | 数字与数字型编码/万能键 | key_filter / input_session | 自动化已回归；最终 TSF 待验 |
| W06 | 二三候选、翻页键、Shift、冲突 | key_filter / input_session | 自动化已回归；最终 TSF 待验 |
| W07 | 标点与系统键盘布局 | key_filter / input_session | 自动化已回归；最终 TSF 待验 |
| W08 | Ctrl/Alt/Win、Shift 切换追踪 | key_filter / input_session | 自动化已回归；最终 TSF 待验 |
| W09 | Windows 中英文/语言栏 | input_session | 自动化已回归；最终 TSF 待验 |
| W10 | 临时入口、菜单、不可用模式 | temporary_input / temporary_loading | 自动化已回归；最终 TSF 待验 |
| W11 | 临时英文 | temporary_input / semantic_modes | 自动化已回归；最终 TSF 待验 |
| W12 | 临时拼音 | pinyin_search / temporary_input | 自动化已回归；最终 TSF 待验 |
| W13 | 空临时模式与退格 | temporary_input | 自动化已回归；最终 TSF 待验 |
| W14 | 反查和候选显示 | reverse_lookup / candidate_contract | 自动化已回归；最终 TSF 待验 |
| W15 | 失焦、宿主终止、成功重载 | input_session / session_contract | 自动化已回归；最终 TSF 待验 |
| W16 | 提交前失败、部分成功、清理失败 | input_session / session_contract | 自动化已回归；最终 TSF 待验 |
| W17 | 无变化/失败/恢复热重载 | runtime_contract / reload | 自动化已回归；最终 TSF 待验 |
| W18 | 用户词库与设置保存 | data_contract / settings save | 自动化已回归；最终 TSF 待验 |
| W19 | 配置/码表/路径兼容 | config_projection / data_contract | 自动化已回归；最终 TSF 待验 |
| W20 | 宿主与系统集成 | Release 编译；TIP/COM/GDI/Velopack 集成待测 | 仅编译；真实集成待验 |
| W21 | TSF 预检准备与同步编辑 | input_session | 自动化已回归；最终 TSF 待验 |
| W22 | 开关/运行时变更遇编辑失败或重入 | input_session / session_contract | 自动化已回归；最终 TSF 待验 |
| W23 | 预编辑类别、None 与候选发布 | input_session / candidate_data_tests | 自动化已回归；最终 TSF 待验 |
| W24 | 校验场景与资源关闭 | runtime_contract / data_contract | 自动化已回归；最终 TSF 待验 |
| A01 | 安装启用与服务创建 | ExternalHostTest | 分项证据，见测试记录 |
| A02 | 主形码与候选 | ExternalHostTest / NativeContractTest | 分项证据，见测试记录 |
| A03 | 持续英文 | ModeHostTest | 分项证据，见测试记录 |
| A04 | 独立临时拼音 | ModeHostTest / NativeContractTest | 分项证据，见测试记录 |
| A05 | 模式切换与旧缓冲 | ModeHostTest / EditorContractTest | 分项证据，见测试记录 |
| A06 | 数字/符号/字面文本 | ModeHostTest / semantic_modes | 分项证据，见测试记录 |
| A07 | 回车与输入类型 | BrowserHostTest | 分项证据，见测试记录 |
| A08 | Unicode 与选区 | BrowserHostTest / EditorContractTest | 分项证据，见测试记录 |
| A09 | 连接与生命周期 | BrowserHostTest / LifecycleContractTest | 分项证据，见测试记录 |
| A10 | 编辑失败 | EditorContractTest / LifecycleContractTest | 分项证据，见测试记录 |
| A11 | 加载与重载 | LifecycleContractTest / ResourceContractTest | 分项证据，见测试记录 |
| A12 | 数据管理 | SettingsUiTest / LibraryContractTest / LibraryModelTest | 分项证据，见测试记录 |
| A13 | 交互和可访问性 | ExternalHostTest / SettingsUiTest；TalkBack/触摸目标/真机振动待验收 | 部分，保留未完成 |
| A14 | 隐私和资源 | BrowserHostTest 密码 + 源码审核；长时间资源增长待测 | 部分，保留未完成 |
| A15 | 升级及设备覆盖 | 覆盖安装、两 ABI 静态对齐；ARM64/16 KB 运行待测 | 部分，保留未完成 |
| A16 | 空临时输入与无效/重复切换 | NativeContractTest / BrowserHostTest | 分项证据，见测试记录 |
| A17 | 宿主能力与预编辑所有权 | EditorContractTest / LifecycleContractTest | 分项证据，见测试记录 |
| A18 | Busy、控制事件、View 与连接 | LifecycleContractTest / BrowserHostTest | 分项证据，见测试记录 |
| R01 | 主表/用户表/辅助表缺失、格式损坏、恢复 | runtime_contract / LifecycleContractTest | 分项证据，见测试记录 |
| R02 | 码表与配置多文件保存部分失败 | data_contract / LibraryModelTest | 分项证据，见测试记录 |
| R03 | 相对路径、跨目录另存、SAF 取消、绝对路径不可达 | data_contract / ResourceContractTest / SettingsUiTest | 分项证据，见测试记录 |
| R04 | 设置与服务并发、旧加载迟到 | LibraryModelTest / LifecycleContractTest | 分项证据，见测试记录 |
| R05 | 内嵌表升级、用户修改、备份恢复 | ResourceContractTest + 覆盖安装 | 分项证据，见测试记录 |
| R06 | 资源包激活中断/重启 | ResourceContractTest 切换前后故障注入、重新 BuildRuntime；非 OS kill/断电测试 | 分项证据，见测试记录 |
| B01 | ABI 版本、错误动作、超长数据、非法 UTF-8 | ffi_protocol / NativeContractTest | 分项证据，见测试记录 |
| B02 | 新接口错误/失效句柄、重复销毁、跨会话误用 | ffi_protocol / NativeContractTest | 分项证据，见测试记录 |
| B03 | 输出 buffer、JNI 字符串/局部引用 | ffi_protocol / EditorContractTest / LibraryContractTest | 分项证据，见测试记录 |
| B04 | 同会话并发、旧 token、销毁中的回调 | ffi_protocol / LifecycleContractTest | 分项证据，见测试记录 |
| B05 | ABI 装载与包体 | check_apk.py + zipalign；ARM64/16 KB 运行未验 | 部分，保留未完成 |
| B06 | Rust/C/JNI 等价与数据接口 | ffi_protocol / NativeContractTest / LibraryContractTest；本次重建 C ABI 冒烟通过 | 分项证据，见测试记录 |

## 后续验收边界

用户确认暂时没有 ARM64 真机。ARM64 真机性能和最终双端联合验收中的相关条目
继续未完成，模拟器或 ELF/ZIP 检查不能替代 16 KB 设备运行。当前模拟器未安装
TalkBack，节点标签检查不等同于实际 TalkBack 操作通过。
