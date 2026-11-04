//! A message-only window owned by the TSF apartment. The renderer sends identities,
//! never COM objects or text; callbacks keep their owner alive during reentrancy.
use std::{ffi::c_void, rc::Rc};
use windows::{
    core::w,
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
};

pub(crate) const SELECT: u32 = WM_APP + 41;
pub(crate) const VERIFY_TIMER: usize = 2;
pub(crate) const MODEL_TIMER: usize = 3;
type Callback = Rc<dyn Fn(u64, usize)>;
pub(crate) struct CandidateDispatch(HWND);
impl CandidateDispatch {
    pub fn new(callback: impl Fn(u64, usize) + 'static) -> windows::core::Result<Self> {
        let instance = HINSTANCE(crate::module_handle() as *mut c_void);
        static REGISTER: std::sync::Once = std::sync::Once::new();
        REGISTER.call_once(|| unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(dispatch),
                hInstance: instance,
                lpszClassName: w!("MyWubiCandidateDispatch"),
                ..Default::default()
            });
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("MyWubiCandidateDispatch"),
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                None,
            )?
        };
        let callback: Box<Callback> = Box::new(Rc::new(callback));
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(callback) as isize);
        }
        Ok(Self(hwnd))
    }
    pub fn hwnd(&self) -> HWND {
        self.0
    }
}
impl Drop for CandidateDispatch {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
unsafe extern "system" fn dispatch(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let data = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Callback;
    match msg {
        WM_TIMER if wp.0 == MODEL_TIMER && !data.is_null() => {
            let _ = KillTimer(Some(hwnd), MODEL_TIMER);
            let callback = (*data).clone();
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(0, 1)));
            LRESULT(0)
        }
        WM_TIMER if wp.0 == VERIFY_TIMER && !data.is_null() => {
            let _ = KillTimer(Some(hwnd), VERIFY_TIMER);
            let callback = (*data).clone();
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(0, 0)));
            LRESULT(0)
        }
        SELECT if !data.is_null() => {
            let callback = (*data).clone();
            // No Rust panic may unwind through the window procedure.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                callback(wp.0 as u64, lp.0 as usize)
            }));
            LRESULT(0)
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !data.is_null() {
                drop(Box::from_raw(data));
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
