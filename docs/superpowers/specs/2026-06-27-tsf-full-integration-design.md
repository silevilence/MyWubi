# TSF 完整集成设计 — MyWubi 输入法

## 概述

补齐 Windows TSF 输入法的所有缺失接口，实现真正可用的文本上屏、模式切换、焦点管理、触摸键盘支持。

对比参考实现 [ime-rs](https://github.com/saschanaz/ime-rs)（Microsoft 官方 TSF Sample IME 的 Rust 移植），本设计覆盖四个子系统。

---

## 1. 文本上屏流水线（P0）

### 1.1 整体数据流

```
ITfKeyEventSink::OnKeyDown (同步, 必须快速返回)
  → key_filter::translate(wparam) → InputEvent
  → StateMachine::handle(event) → Transition
  → apply_transition(t, context) → 构建 EditSessionOp
  → RequestEditSession(TF_ES_ASYNC, DoEditSession)
      └─ DoEditSession(ec):
           match op:
             CompositionUpdate(spelling)  → 更新 composing range, SetText, DisplayAttr
             CommitAndReplace(text)       → 替换 composing text 为汉字, EndComposition
             ClearComposition             → 删除 composing text, EndComposition
             NoOp                         → 无动作
```

### 1.2 设计决策

- **Composition 路径**：完整路径，composing 态显示编码字母（如 "ggll"），选词时替换上屏
- **EditSession**：异步（`TF_ES_ASYNC`），避免重入问题。操作参数通过 `EditSessionOp` enum 传递
- 不使用 `ITfCandidateListUIElement`，候选框继续使用现有的 Win32 分层窗口

### 1.3 Composition 生命周期

| 触发 Transition | 动作 |
|-----------------|------|
| 首次 `Char` → `SpellingUpdated` | `StartComposition(tip_context)`，range 在光标处，`SetText("g")`，`SetDisplayAttr(gaInput)` |
| 后续 `Char` → `SpellingUpdated` | 在现有 range 上 `SetText("ggll")`，保持 `gaInput` |
| 有候选 `Candidates{..}` | 在现有 range 上 `SetText(spelling)`，切换到 `gaConverted` |
| `Space`/`Enter`/`Select` → `Commit` | `SetText(汉字)` 覆盖 composing range，`EndComposition` |
| `Esc` → `Cleared` | 删除 composing range 内容，`EndComposition` |
| `Passthrough` | `EndComposition`（保留已输入编码在文档中） |

### 1.4 ITfCompositionSink

- `OnCompositionTerminated(ec, pComp)`：清理内部 `_pComposition` 引用，重置状态机

### 1.5 ITfDisplayAttributeProvider

注册两个 `TfGuidAtom`：

| GUID Atom | 用途 | 样式 |
|-----------|------|------|
| `gaInput` | 编码输入态 | 灰色点线下划线 |
| `gaConverted` | 有候选态 | 黑色实心下划线 |

使用 `ITfProperty::SetValue(GUID_PROP_ATTRIBUTE, gaAtom)` 设置到 composition range。

### 1.6 ITfEditSession 实现

`EditSessionOp` enum：

```rust
enum EditSessionOp {
    CompositionUpdate { spelling: String, anchor: Option<ScreenPoint> },
    CommitAndReplace { text: String },
    ClearComposition,
    NoOp,
}
```

`DoEditSession` 中：
- 获取 `ITfContext` → `ITfRange`（当前 selection）
- 按 op 类型执行 composition 操作
- `ITfDisplayAttributeMgr` 的 GUID atom 在 `_InitDisplayAttributeGuidAtom` 中注册

### 1.7 Composition 与候选框的关系

Composition 管理**应用内的文本显示**（带下划线的编码串）；候选框**独立渲染**（Win32 分层窗口），两者通过 `ArcSwap<CandidateData>` 解耦。

按键 → 状态机 → 同时更新 composition（via EditSession）和候选框数据（via ArcSwap）。

---

## 2. 模式切换（P1）

### 2.1 设计决策

- 中英切换键由 `config.toml` 的 `switch_key` 字段控制：`"shift"` / `"ctrl_space"`
- 保留 `Shift` 的正常功能（组合键不触发切换，仅单按 Shift 且 key-up 时触发）

### 2.2 ITfCompartment

管理 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE`：
- `true` = 中文模式（当前行为，编码被拦截）
- `false` = 英文模式（所有字母键 `Passthrough`，直接出英文）

通过 `ITfCompartmentMgr::GetCompartment` 获取，`SetValue(VARIANT)` 切换状态。

### 2.3 PreservedKey

在 `Activate` 中注册，`Deactivate` 中反注册：

| switch_key 值 | PreservedKey | 触发方式 |
|---------------|-------------|----------|
| `"shift"` | `VK_SHIFT` + `TF_MOD_ON_KEYUP` | 抬起 Shift（未按组合） |
| `"ctrl_space"` | `VK_SPACE` + `TF_MOD_CONTROL` | Ctrl+Space |

`OnPreservedKey` 收到后：
1. 反转 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE`
2. 重置 `StateMachine::reset()`
3. 隐藏候选框（`CandidateData::hidden`）

### 2.4 语言栏按钮

通过 `ITfLangBarItemMgr::AddItem` 添加一个 `ITfLangBarItemButton`，绑定到 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` compartment，自动反映中英文状态。

Icon：使用系统内置字体图标（`Marlett` 或 `Segoe MDL2 Assets`），避免打包独立 .ico 文件。OffIcon 显示 "中" 字（可用 `DrawText` 生成）、OnIcon 显示 "英" 字。

注册在 `Activate` → `_AddTextProcessorEngine` 中完成，`Deactivate` 中移除。

---

## 3. 激活兼容（P1）

### 3.1 ITfTextInputProcessorEx

windows-rs 0.61 未导出 `ITfTextInputProcessorEx` trait，需手动实现。

`Activate` → 委托到 `ActivateEx(ptim, tid, 0)`。
`ActivateEx` 中记录 `dwFlags` 字段用于：
- `_IsComLess()`：检查 `TF_TMAE_COMLESS`（无 COM 环境）
- `_IsStoreAppMode()`：检查 `TF_TMF_IMMERSIVEMODE`（Windows Store 应用）

在 `OnTestKeyDown` 中判断 `_IsComLess()` 以调整候选框行为——ComLess 模式下候选框不能依赖传统 Win32 窗口（某些现代应用无 HWND），降级为通过 `ITfUIElementMgr` 注册 `ITfCandidateListUIElement`。

> 注：ComLess 模式的候选框降级在当前 spec 中列为后续需求。第一阶段 ComLess 模式下仅提供 composition 级别的文本显示（无弹出候选窗口），用户通过编码直接上屏。

---

## 4. 防御监听（P2）

### 4.1 ITfThreadFocusSink

- `OnSetThreadFocus`：无操作
- `OnKillThreadFocus`：等效 `Esc` 行为 — 重置状态机、隐藏候选框、`EndComposition`（如果有）

使用 `ITfSource::AdviseSink` 注册到线程管理器，`Deactivate` 中反注册。

### 4.2 ITfTextEditSink

- `OnEndEdit`：检查 composition 是否仍有效（range 未被外部修改）。若有外部修改，`_TerminateComposition` + 重置状态机。

在 `OnSetFocus` 时注册到焦点文档管理器，文档管理器变更时更新。

---

## 5. 触摸键盘（P3）

### 5.1 ITfFnGetPreferredTouchKeyboardLayout

单方法实现：

```rust
fn GetLayout(&self) -> Result<(TKBLayoutType, u16)> {
    Ok((TKBLT_OPTIMIZED, TKBL_OPT_SIMPLIFIED_CHINESE_PINYIN))
}
```

在 `ITfFunctionProvider::GetFunction` 中返回此接口。

---

## 6. 代码结构

当前 `text_service.rs` 约 320 行。所有新增接口实现加入后预计 ~800 行。**保持单文件**，按 `impl` 块分区，每个区域标注注释。

```rust
// text_service.rs 结构（顺序）

// ── 类型定义 ──
struct SinkState { .. }
struct TextService { .. }
enum EditSessionOp { .. }

// ── TextService 内部实现 ──
impl TextService { new, clone_self_unknown, release_self_unknown, apply_transition, .. }

// ── ITfTextInputProcessorEx ──
impl ITfTextInputProcessorEx for TextService { Activate, ActivateEx, Deactivate }

// ── ITfThreadMgrEventSink ──
impl ITfThreadMgrEventSink_Impl for TextService_Impl { .. }

// ── ITfKeyEventSink ──
impl ITfKeyEventSink_Impl for TextService_Impl { OnSetFocus, OnTestKeyDown, OnKeyDown, OnPreservedKey, .. }

// ── ITfCompositionSink ──
impl ITfCompositionSink_Impl for TextService_Impl { OnCompositionTerminated }

// ── ITfDisplayAttributeProvider ──
impl ITfDisplayAttributeProvider_Impl for TextService_Impl { EnumDisplayAttributeInfo, GetDisplayAttributeInfo }

// ── ITfTextEditSink ──
impl ITfTextEditSink_Impl for TextService_Impl { OnEndEdit }

// ── ITfThreadFocusSink ──
impl ITfThreadFocusSink_Impl for TextService_Impl { OnSetThreadFocus, OnKillThreadFocus }

// ── ITfFunctionProvider ──
impl ITfFunctionProvider_Impl for TextService_Impl { GetType, GetDescription, GetFunction }

// ── ITfFnGetPreferredTouchKeyboardLayout ──
impl ITfFnGetPreferredTouchKeyboardLayout_Impl for TextService_Impl { GetLayout }

// ── ITfEditSession (辅助结构体) ──
struct EditSession { op: EditSessionOp, service: ... }
impl ITfEditSession_Impl for EditSession_Impl { DoEditSession }
```

`factory.rs` 中不再需要独立创建 `ITfFnGetPreferredTouchKeyboardLayout` 对象——所有接口都由 `TextService` 这一个 COM 对象实现，通过 QI 获取。

`ITfFunctionProvider` 的作用：TSF 系统通过 QI 获取 `IID_ITfFunctionProvider` 后，调用 `GetFunction(GUID_NULL, IID_ITfFnGetPreferredTouchKeyboardLayout, &ppunk)` 返回触摸键盘布局对象。由于 `TextService` 同时实现了这两个接口，`GetFunction` 中只需 `self.QueryInterface(riid, ppunk)` 即可。

---

## 10. 实施阶段

| 阶段 | 内容 | 预计行数 | 里程碑 |
|------|------|---------|--------|
| Phase 1 | P0：Composition + EditSession + DisplayAttr | ~300 行 | 打字能上屏，应用中看到带下划线的编码，选词后出现汉字 |
| Phase 2 | P1：ITfTextInputProcessorEx + Compartment + PreservedKey + LanguageBar | ~200 行 | Shift/Ctrl+Space 切换中英文，语言栏显示状态 |
| Phase 3 | P2：ITfThreadFocusSink + ITfTextEditSink | ~80 行 | 焦点切换/外部编辑时自动清理 |
| Phase 4 | P3：ITfFunctionProvider + ITfFnGetPreferredTouchKeyboardLayout | ~40 行 | 触摸键盘中文布局 |

---

## 7. 新增字段

`TextService` 结构体新增：

```rust
pub struct TextService {
    // ... 现有字段保留 ...

    // Composition
    composition: Mutex<Option<ITfComposition>>,
    ga_input: Mutex<TfGuidAtom>,       // 编码态 display attribute guid atom
    ga_converted: Mutex<TfGuidAtom>,   // 候选态 display attribute guid atom
    is_composing: AtomicBool,

    // Mode switch
    ime_mode: Mutex<bool>,               // true=中文, false=英文
    preserved_key_guids: Mutex<Vec<(GUID, TF_PRESERVEDKEY)>>, // 用于反注册

    // Activation
    activate_flags: Mutex<u32>,          // TF_TMAE_COMLESS | TF_TMF_IMMERSIVEMODE

    // Thread focus sink
    thread_focus_cookie: Mutex<u32>,

    // Language bar
    lang_bar_button: Mutex<Option<ITfLangBarItemButton>>,
}
```

---

## 8. 测试策略

| 层级 | 测试内容 | 方式 |
|------|---------|------|
| 核心层 | `StateMachine::handle` 已有 22 个集成测试 | 保持现有 |
| key_filter | 虚拟键 → InputEvent 映射 | 已有 7 个单测 |
| EditSessionOp | op 构建正确性 | 新增：针对每种 Transition 验证生成的 op |
| composition 生命周期 | Start/Update/End 序列 | 新增：模拟按键序列验证 composition 状态 |
| Compartment | Get/Set/Toggle 正确性 | 新增：验证 compartment 值变化 |
| DisplayAttribute | GUID atom 注册/获取 | 新增：验证 provider 返回正确的 attr info |
| 模式切换 | PreservedKey 注册 / OnPreservedKey | 新增：模拟触发验证 compartment 切换 + reset |
| 路径解析 | dll_directory() 返回正确路径 | 已有的日志输出可验证 |
| DLL 日志 | init() 生成日志文件 | 已有的日志输出可验证 |

---

## 9. 风险与缓解

| 风险 | 缓解 |
|------|------|
| `ITfTextInputProcessorEx` 手动实现可能 vtable 布局错误 | 参考 windows-rs `#[implement]` 宏生成的代码，或使用 `IUnknown` + `QueryInterface` 手动分发 |
| 异步 EditSession 导致输入延迟感 | `TF_ES_ASYNC` 在 TSF 中通常是微秒级调度；候选框已独立渲染，用户感知不到 |
| `ITfDisplayAttributeProvider` 需要自定义 GUID 与系统不冲突 | 使用 `windows::core::GUID::from_u128` 生成唯一 GUID，参考 ime-rs 的做法 |
| `Passthrough` 时 `EndComposition` 保留编码文本 — 可能不是用户期望 | 形码输入法中，如果用户打了编码但放弃输入（点击别处），保留已输入字母是合理行为；`Esc` 则显式清除 |
| `OnCompositionTerminated` 被系统调用时状态不一致 | 在 handler 中重置所有 composition 相关状态并 log warning |
