# TSF Language Bar Button Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a TSF language-bar button that displays and toggles MyWubi's Chinese/English mode while staying synchronized with `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE`.

**Architecture:** Keep the implementation inside the existing `TextService` COM object. Add small pure helpers for testable mode/display decisions, then implement the TSF item/source/compartment interfaces and symmetric activation cleanup. Generate the 16×16 icon with existing Win32 GDI APIs; add no assets or dependencies.

**Tech Stack:** Rust, windows-rs 0.62, TSF COM, Win32 GDI, Cargo tests and Clippy.

---

## File Map

- Modify: `windows/im_engine/src/text_service.rs`
  - Pure display and mode-change helpers
  - Compartment read/write and notification helpers
  - `ITfLangBarItemButton`, `ITfSource`, and `ITfCompartmentEventSink`
  - Activation/deactivation lifecycle
  - Unit tests
- No new source modules, resources, or dependencies.

### Task 1: Add testable mode/display decisions

**Files:**
- Modify: `windows/im_engine/src/text_service.rs`
- Test: `windows/im_engine/src/text_service.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Write failing display-state tests**

```rust
#[test]
fn chinese_mode_uses_chinese_language_bar_text() {
    assert_eq!(lang_bar_display(true), ("中", "中文模式"));
}

#[test]
fn english_mode_uses_english_language_bar_text() {
    assert_eq!(lang_bar_display(false), ("英", "英文模式"));
}

```

- [ ] **Step 2: Run tests and verify RED**

Run:

```powershell
cargo test -p im_engine text_service::tests::chinese_mode_uses_chinese_language_bar_text
```

Expected: compilation fails because `lang_bar_display` does not exist.

- [ ] **Step 3: Add the minimum pure helpers**

```rust
fn lang_bar_display(chinese: bool) -> (&'static str, &'static str) {
    if chinese {
        ("中", "中文模式")
    } else {
        ("英", "英文模式")
    }
}

```

- [ ] **Step 4: Run focused tests and verify GREEN**

Run:

```powershell
cargo test -p im_engine text_service::tests
```

Expected: all `text_service::tests` pass.

### Task 2: Make the TSF compartment the synchronized mode boundary

**Files:**
- Modify: `windows/im_engine/src/text_service.rs`
- Test: `windows/im_engine/src/text_service.rs`

- [ ] **Step 1: Add fields and initialization**

Extend `TextService` with only the required state:

```rust
lang_bar_sink: Mutex<Option<ITfLangBarItemSink>>,
lang_bar_visible: AtomicBool,
compartment_sink_cookie: Mutex<u32>,
lang_bar_registered: AtomicBool,
```

Initialize them in both `with_theme` and `from_runtime`:

```rust
lang_bar_sink: Mutex::new(None),
lang_bar_visible: AtomicBool::new(true),
compartment_sink_cookie: Mutex::new(0),
lang_bar_registered: AtomicBool::new(false),
```

- [ ] **Step 2: Add compartment helpers**

Use the existing `thread_mgr` and saved TSF client ID:

```rust
fn keyboard_openclose_compartment(&self) -> Result<ITfCompartment> {
    let tm = self.thread_mgr.lock().clone().ok_or(E_UNEXPECTED)?;
    let mgr: ITfCompartmentMgr = tm.cast()?;
    unsafe { mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE) }
}

fn read_compartment_mode(&self) -> Result<bool> {
    let value = unsafe { self.keyboard_openclose_compartment()?.GetValue()? };
    bool::try_from(&value)
}

fn write_compartment_mode(&self, chinese: bool) -> Result<()> {
    let tid = self.cookies.lock().tid;
    let value = VARIANT::from(chinese);
    unsafe {
        self.keyboard_openclose_compartment()?
            .SetValue(tid, &value)
    }
}
```

- [ ] **Step 3: Centralize local mode effects**

Add one method that updates local state without rewriting the compartment:

```rust
fn apply_ime_mode(&self, chinese: bool) {
    let mut mode = self.ime_mode.lock();
    if *mode == chinese {
        return;
    }
    *mode = chinese;
    drop(mode);

    if !chinese {
        self.sm.lock().reset();
        self.end_active_composition();
        self.candidate_tx
            .store(Arc::new(CandidateData::hidden(self.theme_snapshot())));
    }
    self.notify_lang_bar();
}
```

Change `toggle_ime_mode()` to compute the next state, write it to the compartment,
and call `apply_ime_mode(next)`. If the compartment write fails, log the error and
still apply the local state so the button remains usable.

- [ ] **Step 4: Implement compartment notifications**

Add `ITfCompartmentEventSink` to `#[implement(...)]` and implement:

```rust
impl ITfCompartmentEventSink_Impl for TextService_Impl {
    fn OnChange(&self, rguid: *const GUID) -> Result<()> {
        if rguid.is_null()
            || unsafe { *rguid } != GUID_COMPARTMENT_KEYBOARD_OPENCLOSE
        {
            return Ok(());
        }
        match self.read_compartment_mode() {
            Ok(chinese) => self.apply_ime_mode(chinese),
            Err(error) => log::warn!("[TSF] 读取中英文 compartment 失败: {error}"),
        }
        Ok(())
    }
}
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test -p im_engine text_service::tests
```

Expected: all focused tests pass.

### Task 3: Implement and register the language-bar button

**Files:**
- Modify: `windows/im_engine/src/text_service.rs`

- [ ] **Step 1: Add TSF language-bar interfaces**

Add `ITfLangBarItemButton`, `ITfLangBarItem`, `ITfSource`, and their implementation
traits to imports and `#[implement(...)]`.

Use the SDK `GUID_LBI_INPUTMODE` item GUID. Modern Windows ignores third-party
input-mode items registered under an arbitrary GUID.

- [ ] **Step 2: Implement the item source**

Accept exactly one `ITfLangBarItemSink`, using cookie `1`:

```rust
impl ITfSource_Impl for TextService_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Ref<'_, windows::core::IUnknown>) -> Result<u32> {
        if riid.is_null() || unsafe { *riid } != ITfLangBarItemSink::IID {
            return Err(CONNECT_E_CANNOTCONNECT.into());
        }
        let unknown = punk.as_ref().ok_or(E_INVALIDARG)?;
        *self.lang_bar_sink.lock() = Some(unknown.cast()?);
        Ok(1)
    }

    fn UnadviseSink(&self, cookie: u32) -> Result<()> {
        if cookie != 1 || self.lang_bar_sink.lock().take().is_none() {
            return Err(CONNECT_E_NOCONNECTION.into());
        }
        Ok(())
    }
}
```

Add:

```rust
fn notify_lang_bar(&self) {
    if let Some(sink) = self.lang_bar_sink.lock().clone() {
        let flags = TF_LBI_ICON | TF_LBI_TEXT | TF_LBI_TOOLTIP | TF_LBI_STATUS;
        if let Err(error) = unsafe { sink.OnUpdate(flags) } {
            log::warn!("[TSF] 刷新语言栏按钮失败: {error}");
        }
    }
}
```

- [ ] **Step 3: Implement item metadata and button behavior**

`ITfLangBarItem_Impl` returns:

```rust
TF_LANGBARITEMINFO {
    clsidService: CLSID_TEXT_SERVICE,
    guidItem: GUID_LBI_INPUTMODE,
    dwStyle: TF_LBI_STYLE_BTN_BUTTON | TF_LBI_STYLE_SHOWNINTRAY,
    ulSort: 0,
    szDescription: utf16_fixed("MyWubi 中英文切换"),
}
```

`GetStatus` returns `TF_LBI_STATUS_HIDDEN` only when `lang_bar_visible` is false.
`Show` stores the flag and sends `TF_LBI_STATUS`. `GetTooltipString` and `GetText`
return the current `lang_bar_display` strings. `OnClick` toggles only for
`TF_LBI_CLK_LEFT`; menu methods return `Ok(())`.

- [ ] **Step 4: Generate a runtime icon**

Add `create_mode_icon(glyph: &str) -> Result<HICON>` beside the other local helpers.
Reuse the GDI setup pattern from `candidate_window.rs`: create a 16×16 32-bit DIB,
select it into a memory DC, draw the glyph centered with a stock GUI font, then call
`CreateIconIndirect`. Restore and delete all temporary GDI objects on every path.
`GetIcon` calls this helper with `lang_bar_display(mode).0`.

- [ ] **Step 5: Register during activation**

After `self_unknown` is available in `ActivateEx`:

```rust
let item_mgr: ITfLangBarItemMgr = unsafe {
    CoCreateInstance(&CLSID_TF_LangBarItemMgr, None, CLSCTX_INPROC_SERVER)?
};
let item: ITfLangBarItem = punk_self.cast()?;
unsafe { item_mgr.AddItem(&item)? };
```

Obtain the keyboard open/close compartment, advise the current object as
`ITfCompartmentEventSink`, store the cookie, then initialize from its current value.
If it has no readable value, write the existing local mode as the initial value.

- [ ] **Step 6: Unregister during deactivation**

Before releasing `thread_mgr` and `self_unknown`:

1. Unadvise the compartment sink using the saved cookie.
2. Recreate `ITfLangBarItemMgr` and call `RemoveItem`.
3. Clear `lang_bar_sink`.
4. Log individual failures and continue cleanup.

- [ ] **Step 7: Compile and fix interface signatures**

Run:

```powershell
cargo check -p im_engine
```

Expected: success with no compiler errors.

### Task 4: Full verification

**Files:**
- Modify only if verification exposes a defect: `windows/im_engine/src/text_service.rs`

- [ ] **Step 1: Format**

```powershell
cargo fmt --all -- --check
```

If it reports formatting differences, run `cargo fmt --all`, then rerun the check.

- [ ] **Step 2: Run package tests**

```powershell
cargo test -p im_engine
```

Expected: all tests pass.

- [ ] **Step 3: Run strict Clippy**

```powershell
cargo clippy -p im_engine --all-targets -- -D warnings
```

Expected: success with no warnings.

- [ ] **Step 4: Confirm the diff boundary**

```powershell
git status --short
git diff --check
git diff -- windows/im_engine/src/text_service.rs
```

Expected: only the planned source file plus the user's pre-existing `ROADMAP.md`
change and this plan are present; no assets or dependency files are added.

- [ ] **Step 5: Windows manual check**

After rebuilding and reinstalling the TIP:

1. Confirm the language bar shows “中”.
2. Click it and confirm it changes to “英”.
3. Confirm letters pass through in English mode.
4. Press the configured mode-switch key and confirm the icon returns to “中”.
5. Disable/enable the TIP and confirm no stale button remains.
