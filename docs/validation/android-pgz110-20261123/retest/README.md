# OPPO PGZ110 安卓实机复测

本轮只复测首轮未通过的 33 个场景。计数单位是场景，每个场景的全部断言均通过后才勾选。实际朗读按要求免测。

本轮通过 **28/33**。合并首轮已通过的 121 个场景，累计 **149/154**。

| 状态 | 编号 | 场景 | 实机结果 |
| --- | ---: | --- | --- |
| [x] 通过 | 1 | ColorOS 地址栏中英混输 | 地址栏获得焦点，实际选词后继续输入英文和标点，读回“这p./”。 [记录](browser-final.jsonl) |
| [x] 通过 | 2 | ColorOS 网页发送、搜索与换行 | 实际点击发送、搜索与换行键，网页回读正文和提交次数。 [记录](browser-all-windows.jsonl) |
| [x] 通过 | 3 | ColorOS 旋转与切换编辑器 | 网页正文在旋转后保留，切换编辑框并从系统设置返回后，均读回了预期的首字。 [记录](browser-final.jsonl) |
| [x] 通过 | 4 | 系统 WebView 旋转与切换编辑器 | 独立宿主保留旋转中的 WebView，检查旋转、换框和切回后的正文。 [记录](4-hideRotateAndSwitchEditorsDoNotReuseOldCandidates.txt) |
| [ ] 未通过 | 5 | ColorOS 六种受限字段及下一项 | ColorOS 密码框由 com.oplus.securitykeyboard 接管，MyWubi 的 g 键不可用。数字、电话、网址、邮箱和下一项这五组通过。 [记录](browser-final.jsonl) |
| [ ] 未通过 | 6 | 系统 WebView 六种受限字段及下一项 | 系统 WebView 的密码框由 ColorOS 安全键盘接管。数字、电话、网址、邮箱和下一项这五组通过。 [记录](6-restrictedFieldsAndNextActionInChrome-130636.txt) |
| [x] 通过 | 7 | ColorOS Unicode 选区、退格与组合光标 | 用真实按键检查 UTF-16 选区、组合字符删除及移动光标后的提交。 [记录](browser-all-windows.jsonl) |
| [ ] 未通过 | 8 | 自定义模板切换、分组上屏与直接输入 | 26/14/9 键模板、布局恢复、分组选词、普通和受限字段已执行。唯一未通过的是原生密码框 inputType=129，没有出现 MyWubi 的 q 键。 [记录](8-realTemplatesKeepGroupsLocalAndReturnToRememberedMode-133941.txt) |
| [x] 通过 | 9 | 假名别名查询与提交 | 本地输入假名别名并点选，核对独立宿主只收到最终结果。 [记录](9-kanaLiteralAliasUsesVisibleCategoryEntryAndCommitsOnlyTheResult-125330.txt) |
| [x] 通过 | 10 | Emoji 和长颜文字：草稿、重载、滚动和上屏 | 换用实际改变内容的码表触发重载。长颜文字在系统返回手势区以内横向滚动，宿主收到完整原串。 [记录](10-realQueryEditsNeverTouchHostAndFinalEmojiAndLongKaomojiStayComplete-135401.txt) |
| [x] 通过 | 11 | 特殊输入浏览网格，横屏大字 | 已对横屏大字号的浏览网格和翻页栏执行可达性断言。 [记录](11-browserLargeLandscape.txt) |
| [x] 通过 | 12 | 横屏大字号组合输入 | 修正横屏键盘高度，重新核对大字号组合输入和目标尺寸。 [记录](12-landscapeLargeFont.txt) |
| [x] 通过 | 13 | 横屏普通字号组合输入 | 按相同尺寸和按键操作门槛验证普通字号。 [记录](13-landscapeNormalFont.txt) |
| [x] 通过 | 14 | 独立编辑框展开候选、长按连删与松手停止 | 通过独立宿主核对候选展开、连删多个字及松手后正文稳定。 [记录](14-composeExpansionAndRepeatStopsOnRelease.txt) |
| [x] 通过 | 15 | 独立编辑框上屏和底部安全区 | 独立宿主回读最终提交，并测量底部按键与系统区域的距离。 [记录](15-shapeInputInSystemSettings-135519.txt) |
| [x] 通过 | 16 | 候选与剪贴板切换时横屏按键位置 | 切换候选及剪贴板前后读取实际键位和尺寸。 [记录](16-landscapeCandidatesStayAboveProportionalKeys.txt) |
| [x] 通过 | 17 | 中英模式、长按符号与一次性大写 | 按系统长按时长验证符号提交，下一次字母仍按一次性大写处理。 [记录](17-modesUseFixedLabelAndSymbolsDoNotConsumeOneShotShift.txt) |
| [x] 通过 | 18 | 双指交替释放 30 键 | 手机收到 60 个触摸事件，10 轮 q、w、q 的 30 次释放逐个匹配。 [记录](18-overlappingAlternatingFingersDoNotDropRepeatedKeys-130055.txt) |
| [x] 通过 | 19 | 按键预览、上滑、长按和取消 | 在实际按键内避开系统侧边返回区，检查显示预览、取消、长按和上滑后的提交次数。 [记录](19-tapPreviewSwipeHoldAndCancellationCommitExactlyOnce-130101.txt) |
| [x] 通过 | 20 | 模型选择、保存、重开和真实宿主预测 | 复用已校验本地模型，界面选择、保存、重开后开关与模型均保留，独立编辑框实际追加“市”组成“北京市”。 [记录](20-downloadChooseSaveReopenAndPredictInExternalEditor-132825.txt) |
| [x] 通过 | 21 | 本地模型指南与显式试运行 | 打开离线指南后，真机使用缓存模型试运行“北京”。界面显示“市”，改变样例后旧结果消失，配置文件前后逐字节一致。 [记录](21-explicitUiSampleRunsWithoutSelectingOrEnablingModel-132015.txt) |
| [x] 通过 | 22 | 真实宿主选区、外部改写及连接重建 | 选区按 UTF-16 偏移核对，外部替换与连接重建均有独立宿主正文和事件回读。 [记录](22-selectionExternalReplacementAndReconnectionUseTheRealHost-125521.txt) |
| [x] 通过 | 23 | 三套键盘 × 五个词的逐键输入和选词 | 三套键盘均完成相同的五个词。15 条中心及偏移触摸轨迹全部从独立宿主读回了预期正文。 [记录](23-identicalWordsOnThreeLayouts-135630.txt) |
| [ ] 未通过 | 24 | 候选实际显示延迟与系统振动 | 振动开关两种状态均通过。三轮从触摸到截取匹配候选画面的 P95 为 380.7、325.0、307.0 ms，P99 为 448.2、342.8、363.7 ms，超出 200/300 ms 门槛。 [记录](24-tapToCapturedCandidateAndVibrationSwitch-135022.txt) |
| [x] 通过 | 25 | 深色规则编辑，原系统字号 | 原系统字号下拉起数字键盘，错误和保存均可达，最终位置值从磁盘读回。 [记录](25-errorAndSaveAreReachableAboveTheNumericKeyboard.txt) |
| [x] 通过 | 26 | 深色规则编辑，ColorOS 系统字号 1.6 倍 | 系统字号 1.6 倍、深色，数字键盘打开时错误和保存按钮均可见。修正位置后保存，从磁盘读回 3。 [记录](26-errorAndSaveAreReachableAboveTheNumericKeyboard-134149.txt) |
| [x] 通过 | 27 | 生成式预测在同一编辑框连续追加两次 | 复用已有 GGUF。真机以图书→作者→出版社完成两步生成，两次点选均追加到同一宿主。 [记录](27-generatedDictionaryExternalWordsAppendInTheRealHost-135837.txt) |
| [x] 通过 | 28 | 图片皮肤、横屏大字、深色底部按键 | 横屏大字与深色图片皮肤同时启用，底行按键高度达到 48 dp。 [记录](28-actualImageSkinSupportsDarkLandscapeAndLargeFont-125251.txt) |
| [x] 通过 | 29 | 五种皮肤、两档字号的候选和触摸 | 在五种皮肤和两档字号下核对候选及每次点按的提交次数。 [记录](29-nativeKeyboardKeepsCandidatesAndTouchTargetsAcrossSkinAndLargeFontChanges-125300.txt) |
| [ ] 未通过 | 30 | 固定特殊输入工作量三轮延迟及整进程内存 | 四组 JNI 延迟和全集构建达标。全集热重载时整进程峰值为 273.9、278.8、291.9 MiB，均超过 256 MiB；加载完成时为 233.9、237.1、271.0 MiB。 [记录](30-threeRunsOnArtWithTheFixedRuleAndCategoryCounts-134953.txt) |
| [x] 通过 | 31 | 候选容量 1/9/10 与位置 9/10/11 及可延长前缀 | 九组候选容量和全局位置组合全部点选日期并回读正确。可延长规则的前缀未被提前提交，延长后也选中正确日期。 [记录](31-globalPositionsAndReachablePrefixCommitToTheExternalEditor-132620.txt) |
| [x] 通过 | 32 | 更新标记：输入中隐藏、空闲显示、旋转及重建 | 实际键盘有候选或展开时隐藏更新提示，上屏后空闲时重新显示。旋转及切换输入法重建后仍可从标记进入设置。 [记录](32-realImeDefersCachedUpdateUntilIdleAcrossRotationAndRecreation-135331.txt) |
| [x] 通过 | 33 | 应用内发起系统升级并逐文件核对保留数据 | 从应用内发起系统安装，版本码 17→19。批准和实装 APK 的 SHA-256 相同，升级后的原数据及缓存检查通过。 [记录](normal-upgrade-final.json) |

## 如何复测

仪器用例执行命令为 `adb -s <device-serial> shell am instrument -w -r -e class org.mywubi.<类名>#<方法名> … org.mywubi.test/androidx.test.runner.AndroidJUnitRunner`。原始日志记录了类名、方法名和完成码。真机专项使用 `physical_acceptance=true`，模型使用 `model_download_reuse=true`、`model_trial_real=true` 和 `generation_chain=books`。本轮复用本地模型及离线构建缓存。

ColorOS 浏览器使用 `python android/tests/oem_browser_acceptance.py <设备序列号> <输出.jsonl> 1 2 3 5 7`，经 USB 端口转发打开本地验收页面。地址栏从原生控件读回，网页从 DOM 读回实际输入、选区与提交。

[全部 154 项当前状态](../case-results.md) · [安装包哈希与单元测试](build-device.json) · [历次仪器结果](runs.jsonl) · [模型实测](native-generation-chains.jsonl)


## 两个性能项目的最后一次实测

| 轮次 | 触摸至匹配候选画面 P95 / P99（ms） | 全集热重载峰值 PSS（MiB） | 加载完成时 PSS（MiB） | 加载用时（秒） |
| --- | ---: | ---: | ---: | ---: |
| 1 | 380.7 / 448.2 | 273.9 | 233.9 | 1.20 |
| 2 | 325.0 / 342.8 | 278.8 | 237.1 | 1.17 |
| 3 | 307.0 / 363.7 | 291.9 | 271.0 | 1.09 |

每轮触摸先预热 5 次，再测 60 次。计时从手指按下开始，直到屏幕截图首次匹配目标候选，随后点选并从独立编辑框读回“这”。读数含截图采样耗时。本轮关闭振动时系统记录没有新增，打开后出现了新的触摸振动记录。原门槛为 P95 ≤ 200 ms、P99 ≤ 300 ms。[180 次采样和振动记录](physical-presentation.json)

全集加载沿用正常输入法的热重载路径，采样范围包括替换运行中码表的整段时间。Rust 现在复用内容相同的码表及其反查索引。优化前同一负载峰值为 331～338 MiB，优化后三轮峰值仍高于 256 MiB。四组检索延迟和 3 秒构建门槛通过。[优化前](si-live-before-native.jsonl) · [优化后](si-device-benchmark.jsonl) · [双 ABI 库摘要](runtime-libraries.json)

## 修复与回归证据

- 修正横屏键盘高度、相邻按键的多指释放和规则编辑器被软键盘遮挡的问题。独立宿主核对了真实提交，ColorOS 系统字号 1.6 倍时，错误提示、保存按钮和保存结果均已验证。[7 项触摸回归](touch-regression.txt) · [三套布局逐词回读](layout-comparison.jsonl) · [大字号规则编辑](rule-font-1.6.json)
- 修正宿主重建连接后外部改写的记录，并在设置保存和重开后核对真实预测提交。模型试运行、两步连续生成均复用本地权重。原“天气”样例在本机未稳定返回第二个候选；用“图书”作起点，两次点选模型生成的词典外候选，宿主逐次读回“图书作者”和“图书作者出版社”。此项验证连续追加行为。[选区与改写](edit-capture.json) · [设置和预测](mi30-saved-prediction.json) · [本机试运行](model-trial.txt)
- 同版本码的旧测试包曾被系统安装器复用。更新文件改用包含版本码和 SHA-256 的名称，并在交给安装器前核验内容。随后从应用内升级 17 → 19，核对批准文件与实装文件的完整 SHA-256，并在新版本内核对保留数据。升级验收后已换回含本轮修复的常规调试版本码 17。[系统升级记录](normal-upgrade-final.json) · [升级后数据断言](33-preserve-final.txt) · [旧缓存迁移和内容校验](update-cache-regression.txt)
- Android JVM 单元测试 14 项、Rust 核心单元测试 94 项及相关集成测试 23 项通过。已比对主程序、测试包和独立宿主在手机与电脑的制品摘要。[制品与测试](build-device.json)

## 设备收尾

已恢复原输入法、配置、字号、明暗和旋转设置，关闭测试应用并撤销 MyWubi 安装其他应用的权限。保留验收宿主与测试包以便复用，按要求保留 USB 供电时常亮。TalkBack 全程关闭，实际朗读按用户要求勾选免测。[设备最终状态](final-device.json)
