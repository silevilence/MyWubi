//! Read only the bounded TSF prefix after checking the host's input scopes.
use parking_lot::Mutex;
use std::{mem::ManuallyDrop, sync::Arc};
use windows::core::{implement, IUnknown, Interface, Result};
use windows::Win32::{Foundation::E_FAIL, System::Com::CoTaskMemFree, UI::TextServices::*};

#[derive(Default)]
pub(crate) struct Prefix {
    pub allowed: bool,
    pub text: Option<String>,
    pub personal: core_engine::personal::Context,
}

#[implement(ITfEditSession)]
struct ReadPrefix {
    context: ITfContext,
    collect: bool,
    result: Arc<Mutex<Prefix>>,
}
impl ReadPrefix {
    fn collect(&self, cookie: u32) -> Result<()> {
        let mut selection = [TF_SELECTION::default()];
        let mut fetched = 0;
        let status = unsafe {
            self.context
                .GetSelection(cookie, TF_DEFAULT_SELECTION, &mut selection, &mut fetched)
        };
        // TSF hands ownership of the interface to the caller.
        let range = unsafe { ManuallyDrop::take(&mut selection[0].range) };
        status?;
        let range = range
            .filter(|_| fetched == 1)
            .ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let selected = !unsafe { range.IsEmpty(cookie)? }.as_bool();
        if unsafe { self.context.GetStatus()? }.dwDynamicFlags & TF_SD_READONLY != 0 {
            return Ok(());
        }
        let property = unsafe { self.context.GetAppProperty(&GUID_PROP_INPUTSCOPE)? };
        let value = unsafe { property.GetValue(cookie, &range)? };
        if !value.is_empty() {
            let scope: ITfInputScope = IUnknown::try_from(&value)?.cast()?;
            let mut scopes = std::ptr::null_mut();
            let mut count = 0;
            let result = unsafe { scope.GetInputScopes(&mut scopes, &mut count) };
            let allowed = result.is_ok()
                && count <= 64
                && (count == 0 || !scopes.is_null())
                && (count == 0
                    || unsafe { std::slice::from_raw_parts(scopes, count as usize) }
                        .iter()
                        .all(|s| *s != IS_PASSWORD && *s != IS_PRIVATE));
            unsafe { CoTaskMemFree(Some(scopes.cast())) };
            if !allowed {
                return Ok(());
            }
        }
        let classic = classic_edit();
        if classic.is_some_and(|hwnd| unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongW(
                hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
            ) & 0x20
                != 0 // ES_PASSWORD
        }) {
            return Ok(());
        }
        self.result.lock().allowed = true;
        if self.collect {
            // Keep both sides in the same read transaction. TSF offsets here are
            // relative to the observed selection, not document-wide positions.
            let right = unsafe { range.Clone()? };
            unsafe { right.Collapse(cookie, TF_ANCHOR_END)? };
            let mut moved = 0;
            unsafe { right.ShiftEnd(cookie, 512, &mut moved, std::ptr::null())? };
            let mut buffer = [0u16; 512];
            let mut length = 0;
            unsafe { right.GetText(cookie, 0, &mut buffer, &mut length)? };
            let right_text = String::from_utf16(&buffer[..length as usize]).ok();
            let selection = if selected {
                length = 0;
                unsafe { range.GetText(cookie, 0, &mut buffer, &mut length)? };
                String::from_utf16(&buffer[..length as usize]).ok()
            } else {
                Some(String::new())
            };
            self.result.lock().personal = core_engine::personal::Context {
                right: right_text,
                selected: selection,
                selection: Some(core_engine::personal::Selection {
                    start: 0,
                    end: if selected { i64::from(length) } else { 0 },
                    unit: core_engine::personal::CoordinateUnit::Utf16,
                    absolute: false,
                }),
                truncated: true,
                ..Default::default()
            };
            if let Some(hwnd) = classic {
                self.result.lock().text = classic_edit_prefix(hwnd);
                return Ok(());
            }
            // Never include composition or text after the caret.
            unsafe { range.Collapse(cookie, TF_ANCHOR_START)? };
            let mut moved = 0;
            unsafe { range.ShiftStart(cookie, -512, &mut moved, std::ptr::null())? };
            let mut text = [0u16; 512];
            let mut length = 0;
            unsafe { range.GetText(cookie, 0, &mut text, &mut length)? };
            self.result.lock().text = core_engine::assistance::context::decode_utf16_prefix(
                &text[..(length as usize).min(text.len())],
            )
            .ok();
        }
        Ok(())
    }
}

/// The TSF bridge for classic Unicode EDIT controls can expose an empty range.
/// Read only the current line before the caret, never text beyond the selection.
/// An unsupported/long line uses confirmed-commit fallback instead.
fn classic_edit() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::UI::{Input::KeyboardAndMouse::GetFocus, WindowsAndMessaging::*};
    unsafe {
        let hwnd = GetFocus();
        let mut class = [0u16; 32];
        let n = GetClassNameW(hwnd, &mut class);
        (n == 4 && class[..4] == [69, 100, 105, 116] && IsWindowUnicode(hwnd).as_bool())
            .then_some(hwnd)
    }
}
fn classic_edit_prefix(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::*,
    };
    unsafe {
        // EM_GETSEL / EM_LINEFROMCHAR / EM_LINEINDEX / EM_GETLINE.
        let mut start = 0u32;
        let mut end = 0u32;
        SendMessageW(
            hwnd,
            0x00B0,
            Some(WPARAM((&mut start as *mut u32) as usize)),
            Some(LPARAM((&mut end as *mut u32) as isize)),
        );
        if start != end {
            return None;
        }
        let line = SendMessageW(hwnd, 0x00C9, Some(WPARAM(start as usize)), Some(LPARAM(0))).0;
        let index = SendMessageW(hwnd, 0x00BB, Some(WPARAM(line as usize)), Some(LPARAM(0))).0;
        let count = (start as i64).checked_sub(index as i64)?;
        if !(0..=512).contains(&count) {
            return None;
        }
        if count == 0 {
            return Some(String::new());
        }
        let mut buffer = [0u16; 512];
        buffer[0] = count as u16;
        let read = SendMessageW(
            hwnd,
            0x00C4,
            Some(WPARAM(line as usize)),
            Some(LPARAM(buffer.as_mut_ptr() as isize)),
        )
        .0;
        if read != count as isize {
            return None;
        }
        core_engine::assistance::context::decode_utf16_prefix(&buffer[..read as usize]).ok()
    }
}
impl ITfEditSession_Impl for ReadPrefix_Impl {
    fn DoEditSession(&self, cookie: u32) -> Result<()> {
        self.collect(cookie)
    }
}
pub(crate) fn read(context: &ITfContext, tid: u32, collect: bool) -> Prefix {
    let result = Arc::new(Mutex::new(Prefix::default()));
    let request: ITfEditSession = windows::core::ComObject::new(ReadPrefix {
        context: context.clone(),
        collect,
        result: Arc::clone(&result),
    })
    .to_interface();
    // A denied read never causes a write or a key failure. Scope failure disables
    // assistance; a text-read failure after scope validation permits commit-only fallback.
    let _ = unsafe { context.RequestEditSession(tid, &request, TF_ES_SYNC | TF_ES_READ) };
    let mut guard = result.lock();
    std::mem::take(&mut *guard)
}
