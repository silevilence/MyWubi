# WindInput 与 Weasel 输入链路实现研究

> 研究日期：2026-09-02
>
> 目的：为 MyWubi 的 Windows TSF 实现保留可直接跳转的上游源码索引，重点覆盖注册/启用、中英文切换、候选、选字，以及中文状态下「进入候选」与「直接上屏」的判定。
>
> 来源限制：输入链路只使用 WindInput、Weasel 及 Weasel 锁定的官方 librime 源码；注册专项另使用 Microsoft 官方 SampleIME/官方文档、ime-rs 和 PIME 安装器。所有 GitHub 链接固定到 commit 及行号，避免以后主分支移动导致定位失效。

## 1. 先区分三个不同层次

后文始终按这三层描述，不能把它们都简称为「启用输入法」：

| 层次 | 问题 | WindInput 中的代表状态 | Weasel 中的代表状态 |
| --- | --- | --- | --- |
| Windows Profile 层 | TIP 是否已注册、是否出现在系统输入法列表、当前线程是否激活该 Profile | COM/TSF Profile、`Activate`/`Deactivate` | COM/TSF Profile、`ActivateEx`/`Deactivate` |
| 引擎模式层 | 当前 Profile 已激活时，按键按中文引擎解释还是按 ASCII/英文透传 | `chinese_mode`、`GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` | Rime `ascii_mode` option、`status.is_ascii_mode` |
| 组合与候选 UI 层 | 是否存在 composition、是否有候选菜单、候选窗是否显示 | `input_buffer`/`candidates`、TSF composition、独立 UI 通道 | Rime composition/menu、TSF composition、`WeaselPanel` |

因此：Profile 已启用不等于中文模式；中文模式也不保证一定显示候选（标点可直接上屏，唯一全码候选也可自动上屏）；composition 存在也不保证有候选（空码、原码兜底、占位组合均可能如此）。

## 2. 研究快照与入口索引

| 仓库 | 固定快照 | 入口 |
| --- | --- | --- |
| WindInput | [`cbd0d80fea8e0380051a7e97c751698e76bf272e`](https://github.com/huanfeng/WindInput/tree/cbd0d80fea8e0380051a7e97c751698e76bf272e) | [`wind_tsf/src/Register.cpp`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/Register.cpp)、[`wind_tsf/src/KeyEventSink.cpp`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp)、[`wind-coordinator`](https://github.com/huanfeng/WindInput/tree/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src) |
| Weasel | [`d73f6295e8252ed2f7b9c12bae32e9001b1afdaa`](https://github.com/rime/weasel/tree/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa)，固定 librime [`1c23358157934bd6e6d6981f0c0164f05393b497`](https://github.com/rime/librime/tree/1c23358157934bd6e6d6981f0c0164f05393b497) | [`WeaselTSF`](https://github.com/rime/weasel/tree/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF)、[`RimeWithWeasel`](https://github.com/rime/weasel/tree/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/RimeWithWeasel)、[`WeaselUI`](https://github.com/rime/weasel/tree/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselUI) |
| Microsoft SampleIME | [`d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c`](https://github.com/microsoft/Windows-classic-samples/tree/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME) | [`Register.cpp`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L11-L26)、[`Server.cpp`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Server.cpp#L261-L284) |
| ime-rs | [`e08aec82a474c14cbd69c0e0e295a9649f5e59ee`](https://github.com/saschanaz/ime-rs/tree/e08aec82a474c14cbd69c0e0e295a9649f5e59ee) | [`rust/ime/src/registry.rs`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/registry.rs#L1-L18)、[`rust/ime/src/dll.rs`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/dll.rs#L12-L38) |
| PIME（仅多架构安装参考） | [`9f6a1e9161b7f609eb1fadf282048c2907da04c9`](https://github.com/EasyIME/PIME/tree/9f6a1e9161b7f609eb1fadf282048c2907da04c9) | [`installer/installer.nsi`](https://github.com/EasyIME/PIME/blob/9f6a1e9161b7f609eb1fadf282048c2907da04c9/installer/installer.nsi#L640-L664)、[`README.md`](https://github.com/EasyIME/PIME/blob/9f6a1e9161b7f609eb1fadf282048c2907da04c9/README.md#L62-L82) |
| Microsoft TSF/Registry 文档源 | [`79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de`](https://github.com/MicrosoftDocs/win32/tree/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF) | [`text-service-registration.md`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/text-service-registration.md#L22-L74)、[`predefined-category-values.md`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/predefined-category-values.md#L9-L55)、[`64-bit-platform-considerations.md`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/64-bit-platform-considerations.md#L75-L118) |

## 3. WindInput

### 3.1 注册、安装与系统 Profile 启用

WindInput 的 `DllRegisterServer` 委托 `RegisterServer()`，后者依次注册 COM server、语言 Profile 和 TSF categories；卸载严格逆序执行。COM server 写入 `HKCR\CLSID\{...}` 与 `InprocServer32`，并声明 `ThreadingModel=Both`。见 [`dllmain.cpp#L67-L75`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/dllmain.cpp#L67-L75)、[`Register.cpp#L29-L72`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/Register.cpp#L29-L72)、[`Register.cpp#L282-L318`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/Register.cpp#L282-L318)。

Profile 注册优先使用 Windows 8+ 的 `ITfInputProcessorProfileMgr::RegisterProfile`，参数 `bEnabledByDefault=TRUE`；创建新接口失败才回退到 `ITfInputProcessorProfiles::Register` + `AddLanguageProfile`。这意味着「注册 Profile」和「默认启用 Profile」在同一次现代 API 调用里完成，不另写一套 `EnableLanguageProfile`。见 [`Register.cpp#L91-L160`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/Register.cpp#L91-L160)。

它还注册 TIP keyboard、secure mode、UI element、input-mode compartment、COM-less、immersive、system tray、显示属性等 categories，目的是让 Profile 在 Win11/UWP/安全宿主中被正确识别。见 [`Register.cpp#L181-L230`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/Register.cpp#L181-L230)。仓库给开发者的手动注册入口仍是 `regsvr32 wind_tsf.dll`，见 [`wind_tsf/README.md#L154-L166`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/README.md#L154-L166)。

边界：正式安装器是独立项目；本仓能直接核实的是 DLL 自注册和 Profile 默认启用，不能从这里推断安装器是否还调用了额外的系统 API。

### 3.2 中英文模式：按键切换与系统状态必须收敛到同一真相

WindInput 同时支持自定义「轻敲 Shift/Ctrl」和系统/宿主写 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` 两条入口。

自定义切换键在 TSF 前置判定中会解析左右 Shift/Ctrl。纯修饰键的 keydown 必须放行给宿主，但同时记下 pending；真正切换在 keyup，且按住超过阈值不切换。`OnTestKeyUp` 与 `OnKeyUp` 都可派发，兼容只回调其中一个的宿主。见 [`KeyEventSink.cpp#L428-L518`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp#L428-L518)、[`KeyEventSink.cpp#L936-L1006`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp#L936-L1006)、[`KeyEventSink.cpp#L1742-L1787`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp#L1742-L1787)。

协调器最终翻转 `state.chinese_mode`；若配置要求，中文标点状态随模式切换；随后按 `commit_on_switch` 规则处置未完成编码、推送状态、刷新工具栏并隐藏候选。见 [`message_handler.rs#L2174-L2208`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator/message_handler.rs#L2174-L2208)。切到英文时即使不提交文字，也需要返回能结束 composition 的动作，否则宿主中的编码会残留，见 [`message_handler.rs#L620-L640`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator/message_handler.rs#L620-L640)。

系统/宿主入口采用严格值语义：`OPENCLOSE=0` 就是英文，`1` 就是中文；相同值是幂等 no-op。发生真实变化时结束 composition，经 IPC 让服务端仲裁，写回本地模式、语言栏和 conversion compartment。见 [`TextService.cpp#L2973-L3003`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/TextService.cpp#L2973-L3003)、[`TextService.cpp#L3213-L3265`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/TextService.cpp#L3213-L3265)、[`TextService.cpp#L3292-L3387`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/TextService.cpp#L3292-L3387)。

英文半角模式通常直接 `PassThrough`，保留宿主的原始 `WM_KEYDOWN` 语义；只有全角、英文自定义标点、自动配对等输入法确实要出字的路径才吃键。CapsLock 也可临时压制中文输入流而不必改掉持久的 `chinese_mode`。见 [`message_handler.rs#L746-L784`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator/message_handler.rs#L746-L784)、[`message_handler.rs#L787-L812`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator/message_handler.rs#L787-L812)。

### 3.3 中文状态下，何时进入候选

普通码元进入统一入口 `accumulate_code_char`：把字符插入 `input_buffer`，先判顶码，再重建候选，最后返回 `UpdateComposition` 并刷新候选 UI。见 [`coordinator.rs#L3724-L3767`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator.rs#L3724-L3767)、[`coordinator.rs#L3853-L3885`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator.rs#L3853-L3885)。

码表引擎的候选来源是「精确查询 + 前缀查询」，同文去重，精确码作为显式排序层级保留；然后排序、截断并返回 `ConvertResult`。见 [`codetable/engine.rs#L308-L407`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-engine/src/codetable/engine.rs#L308-L407)。协调器每次重建会清空旧候选、调用统一 `build_candidates` 后处理管线、重置分页/高亮、展开简繁变体并同步 preedit。见 [`handle_candidate.rs#L1447-L1492`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L1447-L1492)。

「进入候选」在实现上不是单独的布尔开关，而是下面这组状态同时成立：

1. 当前 Profile/编辑上下文允许接管按键；
2. `chinese_mode` 有效且没有被密码框/CapsLock 等临时抑制；
3. 当前键属于方案的首码/续码字符集；
4. 未命中更高优先级的模式触发、URL 夺取、热键或直接提交分支；
5. 引擎/短语/overlay 后处理得到候选或至少得到需要显示的 preedit/模式标记。

候选 UI 为空且输入缓冲也为空时发 `HideCandidates`；否则经过首显坐标闸门后发送结构化 `UiCommand::UpdateCandidates`，包括 preedit、候选、选中项、页码和 caret 坐标。见 [`coordinator.rs#L4319-L4355`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator.rs#L4319-L4355)、[`coordinator.rs#L4619-L4649`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator.rs#L4619-L4649)。UI 线程合并连续候选帧，只渲染最新一帧，调用 `candidate_window.update`、定位并 `show`；隐藏命令同时关闭窗口与菜单。见 [`wind-ui/manager.rs#L347-L437`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-ui/src/manager.rs#L347-L437)。

TSF 组合文本和候选窗是两条通道：服务端的 `UpdateComposition` 应答在 DLL 内更新本地 `_isComposing/_hasCandidates` 并调用 `CTextService::UpdateComposition`；后者通过 `ITfContext::RequestEditSession` 修改 TSF composition。见 [`KeyEventSink.cpp#L2096-L2118`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp#L2096-L2118)、[`TextService.cpp#L5787-L5843`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/TextService.cpp#L5787-L5843)。因此候选窗不能被当成 TSF composition 的所有者。

### 3.4 候选选择与上屏

键盘入口最终统一到 `commit_selected`：

- 空格选择当前页高亮项；无候选但有原码时按配置清空或上屏原码，见 [`message_handler.rs#L1053-L1113`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator/message_handler.rs#L1053-L1113)。
- 数字 `1..9`、`0` 选当前页对应项，越界交给独立 overflow 策略，见 [`handle_candidate.rs#L2557-L2601`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L2557-L2601)。
- 配置的二/三候选键先换算成人类序号对应的页内 offset，再按当前模式分派；纯修饰键轻敲在 keyup 选词，且「有候选选词、无候选才切中英文」。见 [`handle_candidate.rs#L1616-L1703`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L1616-L1703)。
- 候选窗鼠标命中产生 `UiEvent::CandidateSelect`，页内下标换算成全局下标后仍走与数字键相同的 `commit_selected`，见 [`candidate_window.rs#L3038-L3059`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-ui/src/candidate_window.rs#L3038-L3059)、[`handle_candidate.rs#L3435-L3477`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L3435-L3477)。

`commit_selected` 对拼音分段候选有两种结果：若 `consumed_length` 只消费缓冲前缀，就把已选汉字保留在 composition 内、裁掉已消费编码、重建剩余候选并返回 `UpdateComposition`；若消费整串，就生成最终文本、清理组合状态并返回提交动作。见 [`handle_candidate.rs#L2335-L2444`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L2335-L2444)、[`handle_candidate.rs#L2495-L2553`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L2495-L2553)。

DLL 收到普通 `CommitText` 后，用同一个 TSF edit session 将 composition range 替换为最终文字并 `EndComposition`；非按键上下文改用异步 edit session，避免 Word 拒绝同步锁后又走 `SendInput` 造成重复。见 [`KeyEventSink.cpp#L2041-L2088`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp#L2041-L2088)、[`TextService.cpp#L6040-L6175`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/TextService.cpp#L6040-L6175)。

### 3.5 中文状态下，何时不进候选而直接上屏

WindInput 至少有五类直接上屏路径：

| 路径 | 判定 | 结果 |
| --- | --- | --- |
| 中文标点/全角字符 | 当前键不作为码元、模式/标点映射要求输入法出字 | 直接返回提交动作，不建码表候选 |
| 非码元字母 | 中文下 TSF 已决定吃键，但当前方案不接受该字母作码元 | 结束现有组合并由输入法输出该字符，避免“吃了再吐”丢键；见 [`coordinator.rs#L3707-L3721`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator.rs#L3707-L3721) |
| 唯一全码自动上屏 | 开启 `auto_commit_at_full`，码长达到 `min_len`，恰有一个精确候选，且没有更长后继 | 返回 `should_commit/commit_text`，协调器记账、清候选、提交；见 [`codetable/engine.rs#L245-L272`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-engine/src/codetable/engine.rs#L245-L272)、[`codetable/engine.rs#L422-L451`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-engine/src/codetable/engine.rs#L422-L451) |
| 唯一全码短语自动上屏 | 短语开关开启、达到最短码长、唯一精确短语、没有码表或短语更长后继 | 直接提交短语；见 [`handle_candidate.rs#L1108-L1159`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L1108-L1159) |
| 顶码上屏 | 开启 `top_code_commit`，输入超过最大码长，整串无精确匹配且无更长后继 | 提交前 N 码显示首选，余码继续组成新 composition；见 [`codetable/engine.rs#L485-L526`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-engine/src/codetable/engine.rs#L485-L526)、[`coordinator.rs#L3789-L3850`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator.rs#L3789-L3850) |

自动上屏与顶码的关键不是“候选非空”，而是精确性、唯一性、码长和“是否存在更长后继”的组合判定。否则会把用户仍可能继续输入的前缀过早上屏。

顶码还专门处理 TSF 宿主时序：`direct_commit` 在触发键 keydown 内真实提交前缀，把余码 composition 暂存到 keyup 或兜底定时器再打开，避免 Chromium 一类 diff 宿主把“提交 + 同位置立刻重开”合并成重复/替换。见 [`handle_candidate.rs#L2928-L2978`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L2928-L2978)、[`KeyEventSink.cpp#L2258-L2271`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp#L2258-L2271)。这是比“在一个 edit session 里同时结束旧组合并开始新组合”更稳健的宿主兼容策略。

### 3.6 WindInput 调用链速查

```text
Windows 激活 Profile
  -> TSF OnTestKeyDown 决定是否吃键
  -> OnKeyDown / IPC SendKey
  -> Coordinator::handle_key_event
       英文半角 -> PassThrough
       中文码元 -> accumulate_code_char
         -> EngineManager::convert / build_candidates
         -> AutoCommit | Clear | Normal
         -> UpdateComposition + UiCommand::UpdateCandidates
       选词 -> commit_selected
         -> 分段：UpdateComposition
         -> 完整：CommitText
  -> C++ ResponseType 分派
       UpdateComposition -> TSF composition edit session
       CommitText -> SetText + EndComposition
       CommitThenDefer -> keydown 提交，keyup 重开余码组合
```

## 4. Weasel

> 本节所有链接固定到 `d73f6295e8252ed2f7b9c12bae32e9001b1afdaa`。
>
> 职责边界：Weasel 是 Windows TSF/IPC/UI 前端，真正的翻译、候选菜单、`ascii_mode`、选字和自动上屏规则在仓库锁定的 librime 中。为避免把前端行为误当成引擎行为，本节同时引用 Weasel 固定的 librime commit `1c23358157934bd6e6d6981f0c0164f05393b497`；它也是 Rime 官方一手源码。

### 4.1 注册、安装与系统 Profile 启用

Weasel 明确把安装拆成多个动作，而不是把“写注册表”当作完成：

```text
WeaselSetup.exe
  -> 复制各架构 weasel.dll
  -> regsvr32 /s weasel.dll
       -> DllRegisterServer
       -> RegisterServer + RegisterProfiles + RegisterCategories
  -> EnableLanguageProfile + EnableLanguageProfileByDefault
  -> InstallLayoutOrTip("LANGID:CLSID:PROFILE_GUID")
```

安装器处理 x86/x64/ARM 等多架构 DLL 并逐一注册，见 [`WeaselSetup/imesetup.cpp#L141-L245`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselSetup/imesetup.cpp#L141-L245)。`DllRegisterServer` 的三步中任何一步失败都会回滚已经完成的步骤，见 [`WeaselTSF/Server.cpp#L103-L115`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Server.cpp#L103-L115)。COM 注册写 `HKCR\CLSID\...\InprocServer32` 与线程模型，见 [`WeaselTSF/Register.cpp#L195-L245`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Register.cpp#L195-L245)。

Profile 侧使用 `ITfInputProcessorProfileMgr::RegisterProfile` 注册简体、繁体、香港、澳门、新加坡等语言 Profile，并通过 `enable` 参数选择安装目标，见 [`WeaselTSF/Register.cpp#L44-L93`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Register.cpp#L44-L93)。Categories 包括 keyboard TIP、secure mode、UI element、input-mode compartment、COM-less、immersive 和 system tray，见 [`WeaselTSF/Register.cpp#L111-L144`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Register.cpp#L111-L144)。

注册完成后，安装器还显式调用 `EnableLanguageProfile` 和 `EnableLanguageProfileByDefault`，见 [`WeaselSetup/imesetup.cpp#L298-L320`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselSetup/imesetup.cpp#L298-L320)；最后用 `InstallLayoutOrTip` 把 TIP 加到当前用户语言列表，见 [`WeaselSetup/imesetup.cpp#L430-L445`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselSetup/imesetup.cpp#L430-L445)。卸载则先 `ILOT_UNINSTALL`，再注销 DLL/Profile/Category，见 [`WeaselSetup/imesetup.cpp#L471-L523`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselSetup/imesetup.cpp#L471-L523)。

运行时 `ActivateEx` 保存 thread manager/client id，注册 key、thread、text-edit、language-bar、compartment 等 sinks；DLL 通过命名管道连接 `WeaselServer.exe`，而不是在宿主进程内加载 librime。见 [`WeaselTSF/WeaselTSF.cpp#L124-L166`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/WeaselTSF.cpp#L124-L166)、[`WeaselTSF/WeaselTSF.cpp#L226-L282`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/WeaselTSF.cpp#L226-L282)。

### 4.2 中英文模式

Weasel 的引擎真相是 librime option `ascii_mode`，不是某个 TSF 局部布尔值。TSF 把 keydown/keyup 和修饰键状态转为 Rime key event，经 IPC 交给 `RimeWithWeaselHandler::ProcessKeyEvent`，后者调用 `rime_api->process_key()` 再组装响应。见 [`WeaselTSF/KeyEventSink.cpp#L11-L63`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/KeyEventSink.cpp#L11-L63)、[`WeaselTSF/KeyEventSink.cpp#L76-L146`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/KeyEventSink.cpp#L76-L146)、[`RimeWithWeasel.cpp#L264-L292`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/RimeWithWeasel/RimeWithWeasel.cpp#L264-L292)。

轻敲 Shift/Ctrl 的判定在 librime `AsciiComposer`：单键按下并在 500ms 内松开才切换，期间出现其他按键则不是轻敲。配置项 `ascii_composer/switch_key` 还能决定切换时如何处理旧 composition：`inline_ascii`、`commit_text`、`commit_code`、`clear` 或 `noop`。见 [`ascii_composer.cc#L58-L123`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/ascii_composer.cc#L58-L123)、[`ascii_composer.cc#L202-L239`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/ascii_composer.cc#L202-L239)。

语言栏按钮读取当前 `ascii_mode` 后发送启用/禁用 ASCII 命令，服务端落为 `SetOption(session, "ascii_mode", ...)` 并立即刷新 UI；可配置把状态同步到其他会话。见 [`LanguageBar.cpp#L151-L182`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/LanguageBar.cpp#L151-L182)、[`WeaselServerImpl.cpp#L110-L123`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselIPCServer/WeaselServerImpl.cpp#L110-L123)、[`RimeWithWeasel.cpp#L489-L506`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/RimeWithWeasel/RimeWithWeasel.cpp#L489-L506)。返回状态再同步语言栏和 `GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION` 的 native/fullshape 位，见 [`LanguageBar.cpp#L402-L418`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/LanguageBar.cpp#L402-L418)。

Weasel 对 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` 提供两种兼容语义，见 [`Compartment.cpp#L215-L277`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Compartment.cpp#L215-L277)：

- `ToggleImeOnOpenClose=yes`：Open/Close 真正控制 TIP 是否接管；关闭会清理引擎和 TSF composition。
- 默认兼容路径：把系统 Open/Close 事件解释为中英文切换，反转 `ascii_mode` 后把 compartment 恢复为 open。

这与 WindInput 当前的严格值语义不同。它们代表两种可选的产品契约，不能把两边代码片段混搭：一旦选了“值是真相”，就不能又把变化当成无方向的 toggle；一旦选了“始终 open、事件表示 toggle”，宿主查询到的 open 状态就不再等同于中英状态。

### 4.3 候选生成、展示与 composition

Weasel 本身不做码表/拼音检索。librime 的 Processor 链先决定按键是 `Accepted`、`Rejected` 还是 `Noop`；Context 更新后，Engine 重新切分输入、调用 Translator 生成 translation、合成 Menu，再经过 Filter。见 [`engine.cc#L99-L122`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/engine.cc#L99-L122)、[`engine.cc#L154-L232`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/engine.cc#L154-L232)。`Context::IsComposing()` 与 `HasMenu()` 是两个不同判定，见 [`context.cc#L44-L57`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/context.cc#L44-L57)；只有 `HasMenu()` 时 API 才导出当前页候选、选中索引、页码和选字键，见 [`rime_api_impl.h#L209-L275`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime_api_impl.h#L209-L275)。

Weasel 把 `RimeContext.menu` 复制成候选文字、注释、标签、选中项、页码/末页信息，见 [`RimeWithWeasel.cpp#L450-L473`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/RimeWithWeasel/RimeWithWeasel.cpp#L450-L473)。一次响应可同时包含 `commit`、`status`、preedit、candidate context、config 和 style，见 [`RimeWithWeasel.cpp#L741-L896`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/RimeWithWeasel/RimeWithWeasel.cpp#L741-L896)。TSF 侧按 action 解析响应，并把候选上下文写入前端对象，见 [`ResponseParser.cpp#L21-L68`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselIPC/ResponseParser.cpp#L21-L68)、[`ContextUpdater.cpp#L21-L84`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselIPC/ContextUpdater.cpp#L21-L84)。

候选列表对象实现 `ITfCandidateListUIElement`/`Behavior`/`Integratable`，向 TSF 报告候选数、当前选择和字符串；`BeginUIElement` 还允许系统决定是否由 Weasel 自绘。见 [`CandidateList.cpp#L18-L160`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/CandidateList.cpp#L18-L160)、[`CandidateList.cpp#L285-L316`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/CandidateList.cpp#L285-L316)。前端以 `status.composing` 驱动候选窗显隐，见 [`CandidateList.cpp#L201-L218`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/CandidateList.cpp#L201-L218)。自绘窗是 topmost/no-activate/transparent popup；候选背景与高亮用 GDI+，文字用 DirectWrite，见 [`WeaselUI.cpp#L85-L103`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselUI/WeaselUI.cpp#L85-L103)、[`WeaselPanel.cpp#L785-L950`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselUI/WeaselPanel.cpp#L785-L950)。

TSF composition 与候选菜单仍是两层：`status.is_composing` 使 Weasel 建立 composition；启用 inline preedit 时才把 preedit 写进 TSF range。见 [`EditSession.cpp#L41-L49`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/EditSession.cpp#L41-L49)、[`Composition.cpp#L266-L319`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Composition.cpp#L266-L319)。

### 4.4 选字与直接上屏

键盘选字由 librime `Selector` 解释方案的 `select_keys`，否则按 `1..9,0` 换算当前页索引，再调用 `Context::Select()`；选中会设置 segment 的 selected 状态并触发 notifier。见 [`selector.cc#L118-L157`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/selector.cc#L118-L157)、[`selector.cc#L255-L264`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/selector.cc#L255-L264)、[`context.cc#L113-L124`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/context.cc#L113-L124)。空格确认、回车、退格等编辑行为由 Editor 实现，见 [`editor.cc#L189-L217`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/editor.cc#L189-L217)。

鼠标点击先由面板命中候选，见 [`WeaselPanel.cpp#L298-L327`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselUI/WeaselPanel.cpp#L298-L327)；TSF 再发 `SelectCandidateOnCurrentPage` IPC 命令，并模拟一次 `VK_SELECT`，让 commit 重新进入统一的按键响应/EditSession 链，而不是直接从 UI 往宿主塞文字。见 [`CandidateList.cpp#L389-L400`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/CandidateList.cpp#L389-L400)。悬停只 highlight，滚轮负责高亮/翻页，不等同于 commit，见 [`CandidateList.cpp#L402-L449`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/CandidateList.cpp#L402-L449)。

中文状态下进入 composition/候选的典型条件是：输入字符属于方案 `speller/alphabet` 或 delimiter，并通过 initials/finals 校验；Speller 把字符加入 Context 后触发 Compose。见 [`speller.cc#L100-L145`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/speller.cc#L100-L145)。是否真正出现候选仍取决于 Translator/Menu，因此“字符被接受”不等于“候选非空”。

直接上屏要区分三种语义：

1. **ASCII 透传**：`ascii_mode` 且没有 composition 时，AsciiComposer 返回 `kRejected`，Weasel 不吃键，由宿主原生处理，见 [`ascii_composer.cc#L111-L123`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/ascii_composer.cc#L111-L123)。
2. **Rime commit**：引擎把 commit 累积到 session；API 读取后清空未读 commit，见 [`service.cc#L17-L57`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/service.cc#L17-L57)、[`rime_api_impl.h#L305-L320`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime_api_impl.h#L305-L320)。Weasel 将 commit 写入 composition range 并结束旧 composition；若同一响应中 Rime 仍是 composing，则立刻建立下一段 composition，见 [`EditSession.cpp#L6-L60`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/EditSession.cpp#L6-L60)、[`Composition.cpp#L322-L380`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Composition.cpp#L322-L380)。
3. **提交旧组合但当前键继续处理**：Express Editor 的 `DirectCommit()` 先提交已有 composition，再返回 `kRejected`，当前触发键仍可交给后续 processor 或宿主，见 [`editor.cc#L178-L217`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/editor.cc#L178-L217)。

形码自动上屏主要由方案与 librime 决定：`speller/auto_select`、`max_code_length`、`auto_select_pattern` 以及候选覆盖范围共同约束可自动选择的候选，见 [`speller.cc#L46-L55`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/speller.cc#L46-L55)。达到最大码长可确认当前候选；唯一候选且长度/正则满足也可确认，见 [`speller.cc#L148-L186`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/speller.cc#L148-L186)。新码使当前转换失败时，Speller 可复用前一个匹配；若启用 `_auto_commit`，就提交旧编码并让剩余输入继续组合，见 [`speller.cc#L189-L215`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/speller.cc#L189-L215)。最终 segment 已选中且到达输入末尾时，Engine 按 `_auto_commit` 决定 `Context::Commit()` 还是只 `Forward()`，见 [`engine.cc#L259-L280`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/engine.cc#L259-L280)。

### 4.5 Weasel 调用链速查

```text
TSF OnTestKeyDown / OnKeyDown / OnKeyUp
  -> WeaselClient IPC ProcessKeyEvent
  -> WeaselServerImpl
  -> RimeWithWeaselHandler::ProcessKeyEvent
  -> librime process_key
       Processor -> Context input -> Engine::Compose
       -> Segmentor -> Translator -> Menu -> Filter
       -> accepted/rejected + optional commit + composing/menu/status
  -> Weasel response: commit + status + preedit + candidates
  -> TSF EditSession
       commit 非空 -> SetText + EndComposition
       status.composing -> 建立/更新下一段 composition
  -> CandidateList / WeaselPanel 更新或隐藏
```

## 5. 独立仓库：Windows TIP 注册专项参考

本节的证据标签如下：

- **[官方规范]**：Microsoft 的 TSF/Registry 文档，用于解释 API 语义与位数规则。
- **[官方样例事实]**：Microsoft SampleIME 在锁定快照中实际做了什么。样例代码不自动等于完整生产安装器。
- **[项目事实]**：ime-rs、PIME 或 MyWubi 当前源码的实际选择。
- **[推论/风险]**：从前述证据得出的工程判断；不冒充 Windows 的强制规范。

### 5.1 官方 SampleIME：最小注册基线

[官方规范] 文本服务除了标准 COM in-proc server 注册，还必须通过 TSF 注册语言 Profile 和适用的 Category；Microsoft 文档明确给出 `ITfInputProcessorProfiles`/`ITfCategoryMgr` 这两类入口，见 [`text-service-registration.md#L22-L30`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/text-service-registration.md#L22-L30)、[`#L64-L74`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/text-service-registration.md#L64-L74)。

[官方样例事实] SampleIME 的 `RegisterServer()` 只直接写标准 COM 键：

```text
HKCR\CLSID\{SampleIMECLSID}
  (Default) = "Sample IME"                         REG_SZ
  InProcServer32\(Default) = <SampleIME.dll 绝对路径>  REG_SZ
  InProcServer32\ThreadingModel = "Apartment"      REG_SZ
```

键名常量与语义见 [`Register.cpp#L11-L26`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L11-L26)，实际 `RegCreateKeyEx`/`RegSetValueEx` 见 [`Register.cpp#L210-L263`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L210-L263)，`Apartment` 模型定义见 [`Define.h#L11-L13`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Define.h#L11-L13)。SampleIME **没有手写** `HKLM/HKCU\Software\Microsoft\CTF\TIP` 树；它通过 `ITfInputProcessorProfileMgr::RegisterProfile` 交给 TSF，参数包含简中 `LANGID`、Profile GUID、名称、DLL 图标与 `bEnabledByDefault=TRUE`，见 [`Register.cpp#L33-L77`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L33-L77)。卸载 Profile 使用对称的 `UnregisterProfile`，见 [`Register.cpp#L82-L110`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L82-L110)。

[官方样例事实] `DllRegisterServer` 依次执行 COM server、Profile、Category；任一失败就调用完整 `DllUnregisterServer` 回滚。卸载顺序是 Profile、Category、COM server，见 [`Server.cpp#L261-L284`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Server.cpp#L261-L284)；COM 树用递归删除，见 [`Register.cpp#L169-L202`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L169-L202)。

边界：SampleIME 的 `TRUE` 只是 `RegisterProfile` 的“默认启用”参数；代码中没有另行调用 `EnableLanguageProfile`，也没有调用 `InstallLayoutOrTip`。不能因此把 SampleIME 的 DLL 自注册当成一个覆盖所有 Windows 版本、所有用户语言列表的完整安装方案。

### 5.2 注册项/类别精确矩阵

| 项目 | Microsoft SampleIME | ime-rs | MyWubi 当前实现 |
| --- | --- | --- | --- |
| `HKCR\CLSID\{CLSID}` 默认值 | `Sample IME` | `Sample Rust IME` | `TEXT_SERVICE_NAME` |
| `InProcServer32` 默认值 | 当前 DLL 绝对路径 | 当前 DLL 路径 | `im_engine.dll` 绝对路径 |
| `ThreadingModel` | `Apartment` | `Apartment` | `Apartment` |
| 额外 COM 值 | 无 | 无 | `ProgID=MyWubi.TextService.1`、`EnableCompatibleTsf=1`、手写两个 `Implemented Categories` |
| Profile API | `ITfInputProcessorProfileMgr::RegisterProfile(..., TRUE)` | 同一 API，`enabled_by_default=true` | 同一 API，`bEnabledByDefault=true` |
| 手写 `HKLM\...\CTF\TIP` | 否 | 否 | 是：TIP 根、LanguageProfile、Description/Icon/Enable、Category、CLSID 等 |
| 手写 `HKCU\...\CTF\TIP` | 否 | 否 | 是：TIP 根、LanguageProfile、Description/Enable |
| Category API | `ITfCategoryMgr::RegisterCategory`，8 项 | 同一 API，同 8 项 | 同一 API，同 8 项；另手写 2 项 Category 注册表 |
| 显式 `EnableLanguageProfile` | 否 | 否 | 是，注册后再启用 |
| `InstallLayoutOrTip` | 否 | 否 | **否** |
| 卸载目标 | Profile、8 Category、HKCR COM 树 | 同左 | Disable/Profile、8 Category、HKCR COM 树、HKLM CTF/TIP 树；**未删 HKCU CTF/TIP 树** |
| 失败回滚 | `DllRegisterServer` 任一步失败即全量卸载 | Rust 闭包失败即 `DllUnregisterServer` | `install()` 中间失败直接返回，没有完整事务回滚 |
| 权限/位数 | 样例未展示提权与多架构安装编排 | README 要求管理员；仅测 x86-64/ARM64 | HKLM 写入需管理员；未传 `KEY_WOW64_32KEY/64KEY`，当前视图随注册进程位数 |

ime-rs 的价值在于它是 SampleIME 注册逻辑的 `windows-rs`/Rust 映射：Profile 参数见 [`registry.rs#L35-L60`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/registry.rs#L35-L60)，8 个 Category 与对称注销见 [`registry.rs#L79-L112`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/registry.rs#L79-L112)，`winreg` 写 HKCR/CLSID 与 `Apartment` 见 [`registry.rs#L114-L135`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/registry.rs#L114-L135)，回滚见 [`dll.rs#L12-L38`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/dll.rs#L12-L38)。其 README 要求在管理员终端运行 `regsvr32`，并明确仅测 x86-64/ARM64，见 [`README.md#L5-L18`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/README.md#L5-L18)。

MyWubi 矩阵中的事实分别来自 [`registrar.rs#L30-L88`](../../windows/tip_manager/src/registrar.rs#L30)、[`registrar.rs#L90-L189`](../../windows/tip_manager/src/registrar.rs#L90)、[`registrar.rs#L195-L212`](../../windows/tip_manager/src/registrar.rs#L195)、[`profile.rs#L94-L149`](../../windows/tip_manager/src/profile.rs#L94) 与 [`lib.rs#L27-L70`](../../windows/tip_manager/src/lib.rs#L27)。

`registrar::register_tip` 当前实际写入的键值如下。表中 `{CLSID}` 是 Text Service CLSID，`{PROFILE}` 是语言 Profile GUID，`0x00000804` 是简体中文 LANGID。这里记录的是 MyWubi 现状，不代表这些手写 CTF 键都是 Microsoft 要求的最小集合。

| 根键与路径 | 值名/类型 | 当前写入内容 |
| --- | --- | --- |
| `HKCR\CLSID\{CLSID}` | `(Default)` / `REG_SZ` | `TEXT_SERVICE_NAME` |
| 同上 | `EnableCompatibleTsf` / `REG_DWORD` | `1` |
| `HKCR\CLSID\{CLSID}\InprocServer32` | `(Default)` / `REG_SZ` | TIP DLL 完整路径 |
| 同上 | `ThreadingModel` / `REG_SZ` | `Apartment` |
| `HKCR\CLSID\{CLSID}\ProgID` | `(Default)` / `REG_SZ` | `MyWubi.TextService.1` |
| `HKCR\CLSID\{CLSID}\Implemented Categories\{GUID_TFCAT_TIP_KEYBOARD}` | `(Default)` / `REG_SZ` | 空字符串 |
| `HKCR\CLSID\{CLSID}\Implemented Categories\{GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT}` | `(Default)` / `REG_SZ` | 空字符串 |
| `HKLM\SOFTWARE\Microsoft\CTF\TIP\{CLSID}` | `(Default)`、`Display Description` / `REG_SZ` | `TEXT_SERVICE_NAME` |
| 同上 | `EnableCompatibleTsf` / `REG_DWORD` | `1` |
| `...\LanguageProfile` | `(Default)` / `REG_SZ` | `{PROFILE}` |
| `...\LanguageProfile\0x00000804\{PROFILE}` | `Description`、`IconFile` / `REG_SZ` | 服务名、DLL 路径 |
| 同上 | `IconIndex`、`Enable` / `REG_DWORD` | `0`、`1` |
| `...\Category\Category{GUID_TFCAT_TIP_KEYBOARD}` | `(Default)` / `REG_SZ` | 空字符串 |
| `...\Category\Category{GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT}` | `(Default)` / `REG_SZ` | 空字符串 |
| `...\CLSID` | `(Default)` / `REG_SZ` | `{CLSID}` |
| `HKCU\SOFTWARE\Microsoft\CTF\TIP\{CLSID}` | `(Default)` / `REG_SZ` | `TEXT_SERVICE_NAME` |
| `...\LanguageProfile` | `(Default)` / `REG_SZ` | `{PROFILE}` |
| `...\LanguageProfile\0x00000804\{PROFILE}` | `Description` / `REG_SZ`、`Enable` / `REG_DWORD` | 服务名、`1` |

对应实现入口是 [`registrar.rs#L24-L193`](../../windows/tip_manager/src/registrar.rs#L24)；Profile API、显式 `EnableLanguageProfile` 与八个 Category 的入口是 [`profile.rs#L84-L176`](../../windows/tip_manager/src/profile.rs#L84)。诊断时应同时导出注册前/后的 HKCR、HKLM、HKCU 视图，再与 API 查询结果对照，不能只看其中一棵树。

### 5.3 官方 SampleIME 的 8 个 Category 与 MyWubi 能力对齐

[官方规范] 类别是对系统/宿主的能力声明，不是“多注几个总更兼容”的开关。各标准类别的官方简介见 [`predefined-category-values.md#L9-L33`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/predefined-category-values.md#L9-L33)、[`#L50-L55`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/predefined-category-values.md#L50-L55)。SampleIME 的 8 项数组与 `RegisterCategory`/反注册循环见 [`Register.cpp#L17-L26`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L17-L26)、[`#L119-L164`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L119-L164)。

| Category | 官方用途 | MyWubi 声明 | MyWubi 实现对齐状态 |
| --- | --- | --- | --- |
| `GUID_TFCAT_TIP_KEYBOARD` | 标识为键盘 TIP | 是 | **已对齐**：实现 `ITfTextInputProcessorEx`/`ITfKeyEventSink` 键盘输入链 |
| `GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER` | 为文本范围提供显示属性 | 是 | **已对齐**：实现 `ITfDisplayAttributeProvider`、`IEnumTfDisplayAttributeInfo`、`ITfDisplayAttributeInfo` |
| `GUID_TFCAT_TIPCAP_UIELEMENTENABLED` | 支持 TSF UIElement/无 UI 模式 | 是 | **声明与实现需复核**：当前未见 `ITfUIElementMgr`、`BeginUIElement` 或 `ITfCandidateListUIElement`，候选窗为独立 GDI 窗口 |
| `GUID_TFCAT_TIPCAP_SECUREMODE` | 声明可在 secure mode 运行 | 是 | **需复核**：`ActivateEx` 保存激活 flags，未见针对 secure flag 的资源/功能收缩分支 |
| `GUID_TFCAT_TIPCAP_COMLESS` | 支持无 COM 激活 | 是 | **基础对齐**：实现 `ITfTextInputProcessorEx::ActivateEx` 并保存 `TF_TMAE_COMLESS` 等 flags，仍需真实宿主验证 |
| `GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT` | 支持输入模式 compartment | 是 | **已对齐**：读写并订阅 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` |
| `GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT` | 声明可在 Windows Store/沉浸式应用正确运行 | 是 | **需复核**：已有 `ActivateEx`，但外部 GDI 候选窗、文件访问与宿主限制尚需真机证明 |
| `GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT` | 声明可包含在系统托盘/输入指示器 | 是 | **部分对齐**：已实现并注册语言栏模式按钮，仍应在 Win10/11 输入指示器验证 |

UIElement 项不只要“能显示候选窗”。[官方规范] 声明该 Category 的 TIP 必须通过 `ITfUIElementMgr` 显示 UI，让宿主可控制可见性；候选列表需为 UI-less 宿主提供 `ITfCandidateListUIElement`，见 [`uiless-mode-overview.md#L30-L36`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/uiless-mode-overview.md#L30-L36)。因此这一项应被当作 MyWubi 的“声明-实现对齐检查”，本文不断言它必然导致故障。

### 5.4 `EnableLanguageProfile` 与用户语言列表不是同一层

[官方规范] `InstallLayoutOrTip` 的职责是启用指定键盘布局或文本服务，TIP 字符串形式是 `<LangID>:{CLSID}{ProfileGUID}`；`ILOT_UNINSTALL`/`ILOT_DISABLED` 用于卸载/禁用，见 [`installlayoutortip.md#L19-L30`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/installlayoutortip.md#L19-L30)、[`#L49-L67`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/installlayoutortip.md#L49-L67)、[`#L88-L96`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/installlayoutortip.md#L88-L96)。`InstallLayoutOrTipUserReg` 则可针对指定用户注册路径，传 `NULL` 时默认映射 HKCU/HKLM，见 [`installlayoutortipuserreg.md#L19-L33`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/installlayoutortipuserreg.md#L19-L33)、[`#L38-L66`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/installlayoutortipuserreg.md#L38-L66)。

[项目事实] MyWubi 当前执行 `RegisterProfile(..., true)` 后又调用 `EnableLanguageProfile(..., true)`，但没有 `InstallLayoutOrTip`。[推论/风险] 这可以解释“COM/Profile 检测已成功，但当前用户的键盘列表仍不出现或重装后残留”一类分层问题。这是需要通过真机安装/卸载验证的缺口，不是凭文档即可断定的现有 bug。

### 5.5 管理员权限与 32/64 位视图

[官方规范] HKCR 是 `HKLM\Software\Classes` 与 `HKCU\Software\Classes` 的合并视图；写 HKCR 键通常落到 HKLM，但若对应键已在 HKCU 存在，值的落点还可能受合并规则影响，见 [`hkey-classes-root-key.md#L9-L23`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/SysInfo/hkey-classes-root-key.md#L9-L23)。MyWubi 同时直接写 HKLM，所以安装/卸载操作要求管理员是明确的项目选择；ime-rs 和 PIME 也在其部署文档中明确使用管理员 `regsvr32`。

[官方规范] 32 位与 64 位 in-proc TIP DLL 不能互相加载；要覆盖两类宿主，应使用相同 CLSID 并分别注册两个架构版本。32 位 COM 注册会被重定向到 `HKCR\Wow6432Node\CLSID`，而 CTF TIP 配置是共享层，见 [`64-bit-platform-considerations.md#L75-L110`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/64-bit-platform-considerations.md#L75-L110)。通用 Registry API 可用 `KEY_WOW64_64KEY`/`KEY_WOW64_32KEY` 显式访问另一视图，见 [`32-bit-and-64-bit-application-data-in-the-registry.md#L9-L17`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/SysInfo/32-bit-and-64-bit-application-data-in-the-registry.md#L9-L17)。

PIME 是这一点的生产型独立仓库参考：安装器在 x64 系统注册 x64 DLL，在 ARM64 注册 ARM64 DLL，并始终注册 x86 DLL，见 [`installer.nsi#L640-L664`](https://github.com/EasyIME/PIME/blob/9f6a1e9161b7f609eb1fadf282048c2907da04c9/installer/installer.nsi#L640-L664)；卸载对称反注册各架构，见 [`installer.nsi#L760-L781`](https://github.com/EasyIME/PIME/blob/9f6a1e9161b7f609eb1fadf282048c2907da04c9/installer/installer.nsi#L760-L781)。README 也明确 64 位系统需同时注册/卸载 x86 和 x64 DLL，并要管理员权限，见 [`README.md#L62-L82`](https://github.com/EasyIME/PIME/blob/9f6a1e9161b7f609eb1fadf282048c2907da04c9/README.md#L62-L82)。PIME 在本文中只用于证明多架构安装编排，不用它的封装层反推未直接核验的 CTF 键值。

### 5.6 MyWubi 的差异/风险清单与固定源码索引

| 排查问题 | 事实边界 | 首选固定参考 |
| --- | --- | --- |
| COM 类无法加载 | 先核对 CLSID 默认值、对应位数的 `InProcServer32`、`Apartment` | SampleIME [`Register.cpp#L210-L263`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L210-L263)；ime-rs [`registry.rs#L114-L135`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/registry.rs#L114-L135) |
| Profile 已注册但未启用 | 核对 `RegisterProfile` 返回值、`bEnabledByDefault` 与 `EnableLanguageProfile` 结果 | SampleIME [`Register.cpp#L33-L77`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L33-L77)；MyWubi [`profile.rs#L94-L149`](../../windows/tip_manager/src/profile.rs#L94) |
| 设置的用户键盘列表不出现 | Profile enable 不等于已经过 `InstallLayoutOrTip` 维护用户列表 | Microsoft [`installlayoutortip.md#L19-L30`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/installlayoutortip.md#L19-L30)、[`#L88-L96`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/installlayoutortip.md#L88-L96) |
| 一步失败后剩半套注册 | SampleIME/ime-rs 失败后全量反注册；MyWubi `install()` 当前没有等价回滚 | SampleIME [`Server.cpp#L261-L284`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Server.cpp#L261-L284)；ime-rs [`dll.rs#L12-L38`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/dll.rs#L12-L38) |
| 卸载后当前用户残留 | MyWubi 写入 HKCU CTF/TIP，但 `unregister_tip()` 只删 HKCR 与 HKLM CTF/TIP | MyWubi [`registrar.rs#L124-L151`](../../windows/tip_manager/src/registrar.rs#L124)、[`#L195-L212`](../../windows/tip_manager/src/registrar.rs#L195) |
| UI-less/游戏宿主候选 UI 异常 | 已声明 `UIELEMENTENABLED`，但当前未见 UIElement 交涉实现 | Microsoft [`uiless-mode-overview.md#L30-L36`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/uiless-mode-overview.md#L30-L36)；MyWubi [`profile.rs#L153-L176`](../../windows/tip_manager/src/profile.rs#L153) |
| 32/64 位应用中只有一边可用 | 必须为宿主架构提供匹配 DLL，同 CLSID、分别注册 | Microsoft [`64-bit-platform-considerations.md#L75-L118`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/64-bit-platform-considerations.md#L75-L118)；PIME [`installer.nsi#L640-L664`](https://github.com/EasyIME/PIME/blob/9f6a1e9161b7f609eb1fadf282048c2907da04c9/installer/installer.nsi#L640-L664) |

对 MyWubi 最重要的结论是：保留 COM/Profile/Category 这三层的对称检测；把当前手写 HKLM/HKCU CTF 树视为需要单独证明必要性的项目选择，不要把它与官方 SampleIME 的最小基线混为“Windows 强制必写”；并且把用户语言列表、失败回滚、HKCU 卸载清理、Category 能力对齐以及 x86/x64 独立注册作为后续实机验证点。

推荐把安装/卸载验收固定成一条可重复的检查链：

```text
安装前快照
  -> 注册并记录每层 HRESULT/Win32 error
  -> 查询 Profile 注册状态
  -> 查询目标用户启用状态
  -> 检查 Windows 设置与 Win+Space 可见性
  -> 在 x86/x64、桌面/现代、普通/安全输入宿主中激活
  -> 卸载并检查 COM/Profile/Category/用户列表/HKCU/HKLM 均无残留
```

## 6. 两者对照与对 MyWubi 的借鉴

| 方面 | WindInput | Weasel/Rime | 对 MyWubi 的含义 |
| --- | --- | --- | --- |
| 注册与启用 | DLL 自注册时 `RegisterProfile(..., enabled=true)`；安装器不在本仓 | COM、Profile、Category、Enable、InstallLayoutOrTip 明确分层并回滚 | 以官方 SampleIME/ime-rs 的“COM + Profile API + Category API”为最小基线；对 MyWubi 手写 CTF 树、用户列表、回滚、HKCU 清理和多位数注册分层检测 |
| 核心位置 | Rust 服务/协调器是权威；TSF DLL 做宿主适配和 IPC | librime 服务是权威；TSF DLL 做宿主适配和 IPC | MyWubi 已是核心/前端解耦，应继续让自动上屏、选词、模式策略留在 `core_engine::StateMachine` |
| 中英状态 | `chinese_mode`；OPENCLOSE 采用 0/1 严格值语义 | `ascii_mode`；OPENCLOSE 可配置成真开关或“保持 open、事件切 ascii” | 必须先选定产品契约，再让语言栏、compartment、核心状态共同收敛；不要同时采用两种语义 |
| 按键是否吃掉 | `OnTestKeyDown` 前置决定，强调 C++ 吃键集必须被核心出字集覆盖 | librime 返回 handled/rejected；断线/拒绝时 TSF 放行 | MyWubi 要持续保证 `OnTestKeyDown` 与最终 `Transition::Passthrough` 一致，避免“吃了再吐”导致严格宿主丢键 |
| composition 与候选 | 明确分通道：TSF composition + 独立 UI command | Rime context/status + TSF composition + Candidate UI Element/自绘窗 | 核心状态应显式区分 spelling/composition、候选列表和窗口可见性，不能用一个 `has_candidates` 代替全部状态 |
| 候选生成 | EngineManager + 方案引擎 + 协调器后处理 | Processor/Segmentor/Translator/Menu/Filter 插件链 | MyWubi 当前形码规模无需复制 Rime 全插件链，但可借鉴“查询”和“显示后处理/过滤”分层 |
| 键盘选字 | 空格/数字/二三选键最后汇入 `commit_selected` | Selector/Editor 最后汇入 Context Select/Commit | MyWubi 应让所有键盘入口共享同一个按绝对候选索引选择的核心原语 |
| 鼠标选字 | UI event 回协调器，共用 `commit_selected`，再 push 给活动 TSF 客户端 | UI 先调用核心 Select，再用 `VK_SELECT` 回统一 EditSession 链 | 候选窗不能直接写应用文本；必须走核心选择、词频记账、状态复位、TSF commit 的统一链 |
| 直接上屏 | 标点、非码元、唯一全码、短语自动提交、顶码等显式 `KeyAction` | 方案 processor/editor/speller 产生 commit；同一响应可兼有新 composition | `Transition` 应同时表达“本次提交”和“提交后仍在组合/仍有候选”，现有 `CommitThenCandidates/CommitThenSpelling` 方向正确 |
| 顶码/连续组合 | `direct_commit` 用 keydown 真提交、keyup 重开余码组合，专治 diff 宿主 | 同一 EditSession 响应里 commit 后可立即重开 composition | MyWubi 需用 Word/Chromium/WPS 真机验证当前 `CommitThenCandidates` 的 edit-session 时序；若出现双写/替换，WindInput 的 keyup 边界是可直接复用的修法 |
| 候选渲染 | 独立 Rust UI，Windows layered window/host-render | GDI+ + DirectWrite，自绘并兼容 TSF Candidate UI Element | MyWubi 保持既定纯 GDI `UpdateLayeredWindow`，借鉴数据/交互链即可，不照搬渲染技术栈 |

最值得直接采用的不变量：

1. **注册成功不代表用户已可切换到该 TIP**：COM server、Profile、Category、Profile enable、用户语言列表是可单独失败的层。
2. **模式状态只有一个真源**：语言栏、compartment、工具栏是入口或投影，不能各自翻转一份布尔值。
3. **四个状态必须分开**：按键是否消费、composition 是否存在、候选是否存在、是否有 commit。
4. **同一按键可以同时提交旧文本并建立新组合**：协议/`Transition` 不应强迫两者互斥。
5. **所有选字入口必须回核心**：键盘、鼠标、触屏都先选择候选，再统一学习词频、清状态、提交 TSF 文本。
6. **自动上屏属于核心/方案，不属于 TSF**：TSF 只执行 `Transition`，不重新猜唯一候选、码长或顶码条件。
7. **吃键判定必须早、最终出字必须覆盖**：`OnTestKeyDown(TRUE)` 后核心若返回“透传”并不保证宿主能补回原键。

## 7. MyWubi 后续查找清单

实施或排障时，按问题直接跳到这些上游锚点：

| 问题 | 首选参考 |
| --- | --- |
| 确定 COM 注册表的最小正确值 | SampleIME [`Register.cpp#L210-L263`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L210-L263)、ime-rs [`registry.rs#L114-L135`](https://github.com/saschanaz/ime-rs/blob/e08aec82a474c14cbd69c0e0e295a9649f5e59ee/rust/ime/src/registry.rs#L114-L135) |
| Category 应声明哪些/能力是否对齐 | Microsoft [`predefined-category-values.md#L9-L55`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/predefined-category-values.md#L9-L55)、SampleIME [`Register.cpp#L17-L26`](https://github.com/microsoft/Windows-classic-samples/blob/d59e5f1dc9c768615e4e1ab1f0f009e6a3ed747c/Samples/IME/cpp/SampleIME/Register.cpp#L17-L26)、本文 5.3 |
| 卸载后 HKCU 残留/重装状态异常 | MyWubi [`registrar.rs#L124-L151`](../../windows/tip_manager/src/registrar.rs#L124)、[`#L195-L212`](../../windows/tip_manager/src/registrar.rs#L195)；本文 5.6 |
| 32/64 位宿主只有一边能载入 | Microsoft [`64-bit-platform-considerations.md#L75-L118`](https://github.com/MicrosoftDocs/win32/blob/79eaaa46b30bd0efef0d0f5a65fd7d11fdd8e2de/desktop-src/TSF/64-bit-platform-considerations.md#L75-L118)、PIME [`installer.nsi#L640-L664`](https://github.com/EasyIME/PIME/blob/9f6a1e9161b7f609eb1fadf282048c2907da04c9/installer/installer.nsi#L640-L664) |
| TIP 注册后系统列表里没有 | Weasel [`imesetup.cpp#L298-L320`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselSetup/imesetup.cpp#L298-L320)、[`#L430-L445`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselSetup/imesetup.cpp#L430-L445) |
| Win11/UWP/安全宿主无法使用 | WindInput [`Register.cpp#L181-L230`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/Register.cpp#L181-L230)、Weasel [`Register.cpp#L111-L144`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Register.cpp#L111-L144) |
| Shift 轻敲误切/长按误切 | WindInput [`KeyEventSink.cpp#L428-L518`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp#L428-L518)、librime [`ascii_composer.cc#L58-L123`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/ascii_composer.cc#L58-L123) |
| Ctrl+Space/宿主恢复状态异常 | WindInput [`TextService.cpp#L3292-L3387`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/TextService.cpp#L3292-L3387)、Weasel [`Compartment.cpp#L215-L277`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/Compartment.cpp#L215-L277) |
| 候选有但窗口不显示 | WindInput [`coordinator.rs#L4319-L4649`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/coordinator.rs#L4319-L4649)、Weasel [`CandidateList.cpp#L201-L218`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/CandidateList.cpp#L201-L218) |
| 空格/数字/鼠标选字行为不一致 | WindInput [`handle_candidate.rs#L1616-L1703`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L1616-L1703)、[`#L2356-L2553`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L2356-L2553)、Weasel [`CandidateList.cpp#L389-L449`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/CandidateList.cpp#L389-L449) |
| 唯一全码自动上屏时机不对 | WindInput [`codetable/engine.rs#L245-L272`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-engine/src/codetable/engine.rs#L245-L272)、librime [`speller.cc#L148-L186`](https://github.com/rime/librime/blob/1c23358157934bd6e6d6981f0c0164f05393b497/src/rime/gear/speller.cc#L148-L186) |
| 顶码后余码双写/覆盖 | WindInput [`handle_candidate.rs#L2928-L2978`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_input/crates/wind-coordinator/src/handle_candidate.rs#L2928-L2978)、[`KeyEventSink.cpp#L2258-L2271`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/KeyEventSink.cpp#L2258-L2271)、Weasel [`EditSession.cpp#L6-L60`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/EditSession.cpp#L6-L60) |
| 鼠标点候选后 Word 重复/原码残留 | WindInput [`TextService.cpp#L6119-L6154`](https://github.com/huanfeng/WindInput/blob/cbd0d80fea8e0380051a7e97c751698e76bf272e/wind_tsf/src/TextService.cpp#L6119-L6154)、Weasel [`CandidateList.cpp#L389-L400`](https://github.com/rime/weasel/blob/d73f6295e8252ed2f7b9c12bae32e9001b1afdaa/WeaselTSF/CandidateList.cpp#L389-L400) |
