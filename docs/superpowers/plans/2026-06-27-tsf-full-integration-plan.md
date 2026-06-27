# TSF 完整集成实施计划

基于 spec `2026-06-27-tsf-full-integration-design.md`，分 4 个 Phase 实施。

## Phase 1 — 文本上屏流水线（P0）

> 目标：打字能上屏，应用中看到带下划线的编码，选词后出现汉字

### Task 1.1: 添加 `EditSessionOp` 与 `EditSession` 辅助结构体

**文件**：`windows/im_engine/src/text_service.rs`

**内容**：
```rust
/// 异步 EditSession 的操作描述。
enum EditSessionOp {
    /// 创建或更新 composing text（编码字母串）。
    CompositionUpdate { spelling: String },
    /// 将 composing text 替换为最终候选词，结束 composition。
    CommitAndReplace { text: String },
    /// 删除 composing text，结束 composition。
    ClearComposition,
    /// 无操作。
    NoOp,
}

/// ITfEditSession 的 COM 实现。通过 RequestEditSession 异步调度。
#[implement(ITfEditSession)]
struct EditSession {
    op: EditSessionOp,
    /// 指向 TextService 的原始指针（不增加引用计数，生命周期由 Activate/Deactivate 保证）。
    service_ptr: *const TextService,
}
```

**`DoEditSession` 逻辑**：
1. 通过 `service_ptr` 获取 `TextService` 引用
2. 获取焦点 context：`self.thread_mgr.lock().as_ref()?.GetFocus()`
3. 根据 `op` 执行：
   - `CompositionUpdate`：如果 `composition.is_none()`，`StartComposition`；否则在现有 range 上 `SetText(spelling)`，更新 `DisplayAttr(gaInput)`
   - `CommitAndReplace`：`SetText(汉字)` 覆盖 range，`EndComposition`
   - `ClearComposition`：删除 range 文本，`EndComposition`
   - `NoOp`：不执行
4. 通过 `CompositionSink::OnCompositionTerminated` 处理系统终止 composition 的情况

**测试**：新增 `#[cfg(test)] mod edit_session_tests`，针对每种 `EditSessionOp` 验证传递给 `RequestEditSession` 的 op 值。

### Task 1.2: 在 `TextService` 中新增 composition 相关字段

**文件**：`windows/im_engine/src/text_service.rs`

新增 import：
```rust
use windows::Win32::UI::TextServices::{
    ITfComposition, ITfCompositionSink, ITfCompositionSink_Impl,
    ITfContextComposition, ITfDisplayAttributeMgr, ITfDisplayAttributeInfo,
    ITfProperty, GUID_PROP_ATTRIBUTE, TF_ES_ASYNC,
};
use std::sync::atomic::AtomicBool;
```

新增字段：
```rust
composition: Mutex<Option<ITfComposition>>,
ga_input: Mutex<TfGuidAtom>,         // GUID atom，默认 0
ga_converted: Mutex<TfGuidAtom>,     // GUID atom，默认 0
is_composing: AtomicBool,
```

在 `new()` 中初始化为默认值。

### Task 1.3: 实现 `ITfCompositionSink`

**文件**：`windows/im_engine/src/text_service.rs`

在 `#[implement]` 中新增 `ITfCompositionSink`。

```rust
impl ITfCompositionSink_Impl for TextService_Impl {
    fn OnCompositionTerminated(&self, _ec: Ref<'_, TfEditCookie>, _pcomp: Ref<'_, ITfComposition>) -> Result<()> {
        log::warn!("[TSF] Composition 被外部终止");
        *self.composition.lock() = None;
        self.is_composing.store(false, Ordering::Release);
        self.sm.lock().reset();
        Ok(())
    }
}
```

### Task 1.4: 实现 `ITfDisplayAttributeProvider`

**文件**：`windows/im_engine/src/text_service.rs`

在 `#[implement]` 中新增 `ITfDisplayAttributeProvider`。

1. 定义两个 GUID（全局常量）：
```rust
static GUID_DISPLAY_ATTR_INPUT: GUID = GUID::from_u128(0x...);
static GUID_DISPLAY_ATTR_CONVERTED: GUID = GUID::from_u128(0x...);
```

2. `EnumDisplayAttributeInfo`：返回一个 enum（`Vec<TfDisplayAttributeInfo>` 的迭代器包装）
3. `GetDisplayAttributeInfo`：按 guid 匹配返回对应 info
4. 在 `Activate` 中调用 `_InitDisplayAttributeGuidAtom`：通过 `ITfDisplayAttributeMgr` 注册 GUID → atom 映射

**DisplayAttributeInfo 内容**：
- Input：灰色文本 (`COLORREF(0x808080)`)，点线下划线 (`TF_DA_COLOR / TF_DA_STYLE`)
- Converted：黑色文本 (`COLORREF(0x000000)`)，实心下划线

### Task 1.5: 重构 `apply_transition` → 异步 EditSession

**文件**：`windows/im_engine/src/text_service.rs`

当前 `apply_transition` 同步执行。改为：

1. 按 `Transition` 构建 `EditSessionOp`
2. 同时更新候选框数据（`ArcSwap`，保持同步）
3. 将 op 传入 `EditSession`，调用 `context.RequestEditSession(TF_ES_ASYNC, &edit_session)`

新增辅助方法：
```rust
fn schedule_edit_session(&self, op: EditSessionOp, context: &ITfContext) {
    let edit = EditSession { op, service_ptr: self as *const _ };
    let _ = unsafe { context.RequestEditSession(tid, &edit_session_com, TF_ES_ASYNC) };
}
```

（注：`tid` 需存储在 `SinkState` 或独立字段中，当前已在 `SinkState` 中有 `tid` 字段。）

### Task 1.6: 候选框数据更新保持同步

候选框通过 `ArcSwap` 更新在 `apply_transition` 中同步进行（不改动），不与 EditSession 耦合。即：

```
按键 → OnKeyDown
  ├─ [同步] 更新 ArcSwap（候选框立刻刷新）
  └─ [同步] 构建 EditSessionOp
       └─ [异步] DoEditSession: SetText / DisplayAttr / EndComposition
```

---

## Phase 2 — 模式切换与激活兼容（P1）

> 目标：Shift/Ctrl+Space 切换中英文，语言栏显示状态，支持现代应用激活

### Task 2.1: 实现 `ITfTextInputProcessorEx` 手动 vtable

**文件**：`windows/im_engine/src/text_service.rs`

Windows-rs 0.61 未导出该 trait。手动实现：

1. 在 `#[implement]` 中移除 `ITfTextInputProcessor`，替代为手动实现的包含两者的 IUnknown
2. 或者：保留 `ITfTextInputProcessor` 的 `#[implement]`，额外通过 `QueryInterface` 返回 `ITfTextInputProcessorEx` 的 vtable

推荐方案 B（改动更小）：在 `TextService` 的 `impl` 块中添加手动 `ActivateEx` 方法。生成一个静态 vtable 结构体，在 `QueryInterface` 中判断 `IID_ITfTextInputProcessorEx` 时返回该 vtable。

新增字段 `activate_flags: Mutex<u32>`。

### Task 2.2: 实现 `ITfCompartment` 管理

**文件**：`windows/im_engine/src/text_service.rs`

`Activate` 中新增 `_InitCompartment` 步骤：

```rust
fn init_compartment(&self) -> Result<()> {
    let tm = self.thread_mgr.lock();
    let tm = tm.as_ref().ok_or(...)?;
    let cmgr: ITfCompartmentMgr = tm.cast()?;
    let compartment: ITfCompartment = unsafe {
        cmgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)?
    };
    // 初始设为中文模式 (true)
    unsafe { compartment.SetValue(tid, &VARIANT::from(true))? };
    *self.ime_mode.lock() = true;
    Ok(())
}
```

新增字段 `ime_mode: Mutex<bool>`。

`OnTestKeyDown` 中附加检查：若 `ime_mode == false`，返回 `BOOL(0)`（全部 Passthrough）。

### Task 2.3: 实现 PreservedKey 注册

**文件**：`windows/im_engine/src/text_service.rs`

`Activate` 中根据 `config.switch_key` 注册 PreservedKey：

```rust
fn register_preserved_keys(&self, kmgr: &ITfKeystrokeMgr, tid: u32, cfg: &Config) -> Result<()> {
    match cfg.basic.switch_key {
        SwitchKey::Shift => {
            let key = TF_PRESERVEDKEY { uVKey: VK_SHIFT.0 as u32, uModifiers: TF_MOD_ON_KEYUP };
            kmgr.PreserveKey(tid, &GUID_PRESERVED_SHIFT, &key, &desc_wide)?;
        }
        SwitchKey::CtrlSpace => {
            let key = TF_PRESERVEDKEY { uVKey: VK_SPACE.0 as u32, uModifiers: TF_MOD_CONTROL };
            kmgr.PreserveKey(tid, &GUID_PRESERVED_CTRL_SPACE, &key, &desc_wide)?;
        }
        _ => {}
    }
    Ok(())
}
```

`OnPreservedKey` 中：
1. 翻转 `ime_mode`
2. 翻转 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE`
3. `sm.lock().reset()`
4. `candidate_tx.store(Arc::new(CandidateData::hidden(...)))`

### Task 2.4: 实现语言栏按钮

**文件**：`windows/im_engine/src/text_service.rs`

实现 `ITfLangBarItemButton` 的 COM 对象（可以是 `TextService` 的嵌套结构体或独立类型）。

使用 `ITfLangBarItemMgr::AddItem` 注册。绑定到 `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` 通过 `ITfSource::AdviseSink(IID_ITfCompartmentEventSink)` 监听状态变化后更新按钮图标。

**注意**：语言栏按钮作为独立的 COM 对象可能更适合（类似 ime-rs 的 `LangBarItemButton`）。但在第一阶段可以简化——如果语言栏按钮实现复杂度较高（需要图标资源、事件处理），可降级为仅通过 compartment 状态同步到候选框。

---

## Phase 3 — 防御监听（P2）

> 目标：焦点切换/外部编辑时自动清理

### Task 3.1: 实现 `ITfThreadFocusSink`

**文件**：`windows/im_engine/src/text_service.rs`

在 `#[implement]` 中新增 `ITfThreadFocusSink`。

```rust
impl ITfThreadFocusSink_Impl for TextService_Impl {
    fn OnSetThreadFocus(&self) -> Result<()> { Ok(()) }
    fn OnKillThreadFocus(&self) -> Result<()> {
        log::debug!("[TSF] 线程焦点丢失，清理状态");
        self.sm.lock().reset();
        self.candidate_tx.store(Arc::new(CandidateData::hidden(theme)));
        if self.is_composing.load(Ordering::Acquire) {
            // schedule ClearComposition edit session
        }
        Ok(())
    }
}
```

在 `Activate` 中通过 `ITfSource::AdviseSink(IID_ITfThreadFocusSink)` 注册，`Deactivate` 中反注册。

### Task 3.2: 实现 `ITfTextEditSink`

**文件**：`windows/im_engine/src/text_service.rs`

在 `#[implement]` 中新增 `ITfTextEditSink`。

```rust
impl ITfTextEditSink_Impl for TextService_Impl {
    fn OnEndEdit(&self, pic: Ref<'_, ITfContext>, ec: TfEditCookie, _peditrecord: Ref<'_, ITfEditRecord>) -> Result<()> {
        // 如果有 active composition，检查 range 是否仍有效
        // 若无效，清理状态
        Ok(())
    }
}
```

在 `OnSetFocus`（ITfThreadMgrEventSink）中注册到焦点文档管理器。

---

## Phase 4 — 触摸键盘（P3）

> 目标：触摸键盘显示中文布局

### Task 4.1: 实现 `ITfFnGetPreferredTouchKeyboardLayout`

**文件**：`windows/im_engine/src/text_service.rs`

```rust
impl ITfFnGetPreferredTouchKeyboardLayout_Impl for TextService_Impl {
    fn GetLayout(&self) -> Result<(TKBLayoutType, u16)> {
        Ok((TKBLT_OPTIMIZED, TKBL_OPT_SIMPLIFIED_CHINESE_PINYIN))
    }
}
```

### Task 4.2: 注册 `ITfFunctionProvider`

在 `#[implement]` 中新增 `ITfFunctionProvider`。

```rust
impl ITfFunctionProvider_Impl for TextService_Impl {
    fn GetType(&self) -> Result<GUID> { Ok(GUID::zeroed()) }
    fn GetDescription(&self) -> Result<BSTR> { Err(E_NOTIMPL.into()) }
    fn GetFunction(&self, rguid: &GUID, riid: &GUID, ppunk: *mut *mut c_void) -> Result<()> {
        if *rguid == GUID_NULL && *riid == IID_ITfFnGetPreferredTouchKeyboardLayout {
            // QI self and return
            unsafe { self.QueryInterface(riid, ppunk) }
        }
    }
}
```

---

## 测试策略

| Phase | 测试文件 | 测试内容 |
|-------|---------|---------|
| P1 | `text_service.rs` `#[cfg(test)]` | `EditSessionOp` 构建正确性、`apply_transition` → op 映射 |
| P1 | 新增 `edit_session_tests.rs` | mock composition 生命周期 start → update → commit → terminate |
| P2 | `text_service.rs` `#[cfg(test)]` | PreservedKey 注册/反注册参数正确性 |
| P3 | 现有测试不破坏 | `cargo test -p im_engine` 全部通过 |
| P4 | — | 单行实现，无需独立测试 |

**测试约束**：真正的 TSF COM 对象需要系统运行环境，单元测试仅覆盖 Rust 逻辑层（op 构建、状态转换）。Composition 的生命周期正确性通过现场实测日志验证。

---

## 依赖与顺序

```
Phase 1 (P0)
   ├── 1.1 EditSessionOp + EditSession
   ├── 1.2 新增 composition 字段
   ├── 1.3 ITfCompositionSink
   ├── 1.4 ITfDisplayAttributeProvider   ← 依赖 1.2
   ├── 1.5 重构 apply_transition         ← 依赖 1.1, 1.2
   └── 1.6 候选框同步保持                ← 依赖 1.5

Phase 2 (P1)
   ├── 2.1 ITfTextInputProcessorEx       ← 独立
   ├── 2.2 ITfCompartment                ← 独立
   ├── 2.3 PreservedKey                  ← 依赖 2.2
   └── 2.4 LanguageBar                   ← 依赖 2.2

Phase 3 (P2)
   ├── 3.1 ITfThreadFocusSink            ← 依赖 Phase 1 的 composition 清理
   └── 3.2 ITfTextEditSink               ← 依赖 Phase 1 的 composition 清理

Phase 4 (P3)
   └── 4.1 + 4.2 ITfFnGetPreferredTouchKeyboardLayout ← 独立
```

每个 Phase 内任务可顺序执行；Phase 之间建议按 1→2→3→4 顺序。

---

## 风险备忘

1. **ITfTextInputProcessorEx vtable**：手动实现需要精确匹配 COM ABI。备选方案是直接放弃该接口，仅支持 `ITfTextInputProcessor`（损失 UWP 兼容）。
2. **EditSession 中的 context 生命周期**：`RequestEditSession` 后 context 可能已经改变。在 `DoEditSession` 中需要重新 `GetFocus()` 获取当前 context。
3. **语言栏图标**：若无内嵌图标资源，可用纯色块 + 文字方式或直接省略语言栏支持（保留 compartment + key 切换即可）。
4. **CandidateWindow 线程 + EditSession 线程**：候选框在独立线程，EditSession 在 TSF 线程——两者通过 `ArcSwap` 解耦，无竞争。
