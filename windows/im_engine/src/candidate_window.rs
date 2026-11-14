//! Win32 透明分层候选框窗口。
//!
//! 独立线程 + `WS_EX_LAYERED` + `UpdateLayeredWindow` 贴图 + `ArcSwap` 无锁
//! 跨线程读取 + 16ms 定时器轮询。

use std::ffi::c_void;
use std::mem;
use std::sync::mpsc;
use std::thread::JoinHandle;

use arc_swap::ArcSwap;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::candidate_data::CandidateData;

/// 窗口类名（使用 w!() 宏）。
const CLASS_NAME: windows::core::PCWSTR = w!("MyWubiCandidateWindow");

const TIMER_ID: usize = 1;
const SET_OWNER: u32 = WM_APP + 1;
const TIMER_INTERVAL_MS: u32 = 16;
const SHUTDOWN_TIMEOUT_MS: u64 = 500;

/// `GWLP_USERDATA` 中嵌入的实例数据。
struct WindowData {
    data_src: std::sync::Arc<ArcSwap<CandidateData>>,
    target: HWND,
    hits: std::cell::RefCell<Vec<(RECT, u64, usize)>>,
    pressed: std::cell::Cell<Option<(u64, usize)>>,
    renderer: std::cell::RefCell<crate::candidate_renderer::Renderer>,
}

/// 透明候选框窗口句柄（拥有窗口线程生命周期）。
pub struct CandidateWindow {
    join_handle: Option<JoinHandle<()>>,
    hwnd: HWND,
}

impl CandidateWindow {
    pub fn is_running(&self) -> bool {
        self.join_handle
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
            && unsafe { IsWindow(Some(self.hwnd)).as_bool() }
    }

    /// 在窗口线程中更新宿主，确保候选框位于 Windows 搜索等沉浸式窗口之上。
    pub fn set_owner(&self, owner: HWND) {
        unsafe {
            let _ = PostMessageW(
                Some(self.hwnd),
                SET_OWNER,
                WPARAM(0),
                LPARAM(owner.0 as isize),
            );
        }
    }

    /// 在独立线程中启动候选框窗口。
    pub fn spawn(
        data_src: std::sync::Arc<ArcSwap<CandidateData>>,
        owner: HWND,
        target: HWND,
    ) -> Self {
        let (hwnd_tx, hwnd_rx) = mpsc::channel::<isize>();
        let owner = owner.0 as isize;
        let target = target.0 as isize;
        // 安全创建窗口线程——失败时不 panic，避免崩溃宿主进程
        let join_handle = match std::thread::Builder::new()
            .name("MyWubiCandidateThread".into())
            .spawn(move || run_window_thread(data_src, hwnd_tx, owner, target))
        {
            Ok(h) => Some(h),
            Err(e) => {
                log::error!("候选窗口线程启动失败: {e}");
                return Self {
                    join_handle: None,
                    hwnd: HWND::default(),
                };
            }
        };
        let hwnd = match hwnd_rx.recv() {
            Ok(raw) => HWND(raw as *mut c_void),
            Err(e) => {
                log::error!("候选窗口线程未能发送 HWND: {e}");
                // join_handle 已有效但窗口未创建——join 之
                if let Some(h) = join_handle {
                    let _ = h.join();
                }
                return Self {
                    join_handle: None,
                    hwnd: HWND::default(),
                };
            }
        };
        Self { join_handle, hwnd }
    }

    /// 发送退出信号（WM_CLOSE, 最多重试 3 次）并等待线程结束（超时后分离）。
    pub fn shutdown(&mut self) {
        if self
            .join_handle
            .as_ref()
            .is_some_and(|thread| thread.is_finished())
        {
            if let Some(thread) = self.join_handle.take() {
                let _ = thread.join();
            }
            return;
        }
        if self.hwnd.0.is_null() || self.join_handle.is_none() {
            // 窗口未成功创建，无需关闭
            return;
        }
        if self.join_handle.is_some() {
            // 尝试发送关闭消息（最多重试 3 次，因为窗口线程可能还未初始化完成）
            for _ in 0..3 {
                unsafe {
                    let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if let Some(handle) = self.join_handle.take() {
                let deadline = std::time::Instant::now()
                    + std::time::Duration::from_millis(SHUTDOWN_TIMEOUT_MS);
                while std::time::Instant::now() < deadline {
                    if handle.is_finished() {
                        let _ = handle.join();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                // 超时：分离线程避免阻塞 Deactivate
                log::warn!(
                    "候选窗口: 线程未在 {}ms 内退出，分离线程",
                    SHUTDOWN_TIMEOUT_MS
                );
                // handle 被 drop，线程分离（OS 回收资源）
            }
        }
    }
}

impl Drop for CandidateWindow {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ── 窗口线程 ──────────────────────────────────────────────────────

fn run_window_thread(
    data_src: std::sync::Arc<ArcSwap<CandidateData>>,
    hwnd_tx: mpsc::Sender<isize>,
    owner: isize,
    target: isize,
) {
    let hinstance = HINSTANCE(crate::module_handle() as *mut c_void);
    if hinstance.0.is_null() {
        log::error!("候选窗口: module_handle 未初始化");
        return;
    }

    // 窗口类只需注册一次（整个进程生命周期内有效）。
    // TSF 在同一进程中可能多次 Activate/Deactivate，重复注册会失败。
    static CLASS_REGISTERED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    static REGISTER_ONCE: std::sync::Once = std::sync::Once::new();
    REGISTER_ONCE.call_once(|| {
        let class_name = CLASS_NAME;
        let wc = WNDCLASSW {
            style: WNDCLASS_STYLES(CS_HREDRAW.0 | CS_VREDRAW.0),
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: HICON::default(),
            hCursor: HCURSOR::default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: windows::core::PCWSTR::null(),
            lpszClassName: class_name,
        };
        let ok = unsafe { RegisterClassW(&wc) } != 0;
        CLASS_REGISTERED.store(ok, std::sync::atomic::Ordering::Release);
        if !ok {
            let err = unsafe { windows::Win32::Foundation::GetLastError() };
            log::error!("候选窗口: RegisterClassW 失败, err={err:?}");
        }
    });
    if !CLASS_REGISTERED.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }

    // 创建窗口数据
    let window_data = Box::new(WindowData {
        data_src,
        target: HWND(target as *mut c_void),
        hits: Default::default(),
        pressed: Default::default(),
        renderer: Default::default(),
    });
    let window_data_ptr = Box::into_raw(window_data);

    // 创建分层窗口
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(
                WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0,
            ),
            CLASS_NAME,
            windows::core::PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            Some(HWND(owner as *mut c_void)),
            None,
            Some(hinstance),
            None,
        )
    } {
        Ok(h) => h,
        Err(e) => {
            log::error!("候选窗口: CreateWindowExW 失败: {e}");
            unsafe {
                let _ = Box::from_raw(window_data_ptr);
            }
            return;
        }
    };

    // 注意：class_name 变量在 Once 闭包内作用域，此处使用静态常量 CLASS_NAME。
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, window_data_ptr as isize);
    }
    unsafe {
        let _ = SetTimer(Some(hwnd), TIMER_ID, TIMER_INTERVAL_MS, None);
    }

    // 将 HWND 发回给 CandidateWindow，以便 shutdown 时可以发消息。
    let _ = hwnd_tx.send(hwnd.0 as isize);

    // 消息泵（panic 安全）
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }));
    if let Err(e) = result {
        log::error!("候选窗口消息泵 panic: {:?}", e);
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }

    // 清理
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID);
        let _ = DestroyWindow(hwnd);
    }
}

// ── 窗口过程 ──────────────────────────────────────────────────────

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCHITTEST => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData;
            if !ptr.is_null() {
                let mut point = POINT {
                    x: (lparam.0 as u16) as i16 as i32,
                    y: ((lparam.0 >> 16) as u16) as i16 as i32,
                };
                if ScreenToClient(hwnd, &mut point).as_bool()
                    && (*ptr).hits.borrow().iter().any(|(r, _, _)| {
                        point.x >= r.left
                            && point.x < r.right
                            && point.y >= r.top
                            && point.y < r.bottom
                    })
                {
                    return LRESULT(HTCLIENT as isize);
                }
            }
            LRESULT(HTTRANSPARENT as isize)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN | WM_LBUTTONUP => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData;
            if !ptr.is_null() {
                let data = &*ptr;
                let x = (lparam.0 as u16) as i16 as i32;
                let y = ((lparam.0 >> 16) as u16) as i16 as i32;
                let hit = data
                    .hits
                    .borrow()
                    .iter()
                    .find(|(r, _, _)| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
                    .map(|(_, q, id)| (*q, *id));
                if msg == WM_LBUTTONDOWN {
                    data.pressed.set(hit);
                    if hit.is_some() {
                        windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
                    }
                } else {
                    let pressed = data.pressed.take();
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                    if let Some((query, id)) = pressed.filter(|p| Some(*p) == hit) {
                        if id == crate::candidate_renderer::SCROLL_UP
                            || id == crate::candidate_renderer::SCROLL_DOWN
                        {
                            data.renderer.borrow_mut().scroll(
                                if id == crate::candidate_renderer::SCROLL_UP {
                                    -120.0
                                } else {
                                    120.0
                                },
                            );
                        } else if !data.target.is_invalid() {
                            let _ = PostMessageW(
                                Some(data.target),
                                crate::candidate_dispatch::SELECT,
                                WPARAM(query as usize),
                                LPARAM(id as isize),
                            );
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData;
            if !ptr.is_null() {
                let delta = ((wparam.0 >> 16) as u16) as i16;
                (*ptr).renderer.borrow_mut().scroll(-f32::from(delta));
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData;
            if !ptr.is_null() {
                (*ptr).pressed.set(None);
            }
            LRESULT(0)
        }
        SET_OWNER => {
            if GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) != lparam.0 {
                SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, lparam.0);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_ID => {
            handle_timer(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            // 宿主销毁时系统也会销毁 owned window，不能等消息泵退出后再取数据。
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ── 定时器处理 ────────────────────────────────────────────────────

fn handle_timer(hwnd: HWND) {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData };
    if ptr.is_null() {
        return;
    }
    let window_data = unsafe { &*ptr };

    if window_data.pressed.get().is_some() {
        return;
    }
    let data = window_data.data_src.load_full();

    if data.visible {
        // 实际文字尺寸、位置和贴图由 UpdateLayeredWindow 一次性提交。
        // 先用估算尺寸 SetWindowPos 会在每轮定时器中裁切右侧候选。
        let rendered =
            window_data
                .renderer
                .borrow_mut()
                .draw(hwnd, data, &mut window_data.hits.borrow_mut());
        if let Err(error) = rendered {
            window_data.hits.borrow_mut().clear();
            log::error!("候选窗口系统文字渲染失败: {error}");
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            return;
        }

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
    } else {
        window_data.renderer.borrow_mut().reset();
        window_data.hits.borrow_mut().clear();
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

// ── 辅助函数 ──────────────────────────────────────────────────────

pub(crate) fn get_monitor_rect(
    hwnd: HWND,
    anchor: Option<crate::candidate_data::ScreenPoint>,
) -> (i32, i32, i32, i32) {
    unsafe {
        let monitor = anchor
            .map(|point| {
                MonitorFromPoint(
                    POINT {
                        x: point.x,
                        y: point.y,
                    },
                    MONITOR_DEFAULTTONEAREST,
                )
            })
            .unwrap_or_else(|| MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST));
        let mut info = MONITORINFO {
            cbSize: mem::size_of::<MONITORINFO>() as u32,
            ..mem::zeroed()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let r = info.rcWork;
            (r.left, r.top, r.right, r.bottom)
        } else {
            (
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static RESIZES: std::cell::RefCell<Vec<(i32, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    unsafe extern "system" fn observe_size(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_WINDOWPOSCHANGING {
            let position = &*(lparam.0 as *const WINDOWPOS);
            if !position.flags.contains(SWP_NOSIZE) {
                RESIZES.with_borrow_mut(|sizes| sizes.push((position.cx, position.cy)));
            }
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    #[test]
    fn unchanged_candidates_never_resize_to_a_clipped_intermediate_width() {
        use crate::candidate_data::{CandidateItem, ScreenPoint};
        let data = CandidateData {
            visible: true,
            anchor: Some(ScreenPoint { x: 100, y: 100 }),
            items: ["节", "期间", "长假", "个", "下"]
                .into_iter()
                .enumerate()
                .map(|(index, text)| CandidateItem {
                    expression_kind: None,
                    disabled: false,
                    label: format!("{}.", index + 1),
                    text: text.into(),
                    hint: String::new(),
                })
                .collect(),
            ..Default::default()
        };
        let window_data = WindowData {
            data_src: std::sync::Arc::new(ArcSwap::from_pointee(data)),
            target: HWND::default(),
            hits: Default::default(),
            pressed: Default::default(),
            renderer: Default::default(),
        };
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("MyWubi render regression"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &window_data as *const _ as isize);
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, observe_size as *const () as isize);
            RESIZES.with_borrow_mut(Vec::clear);
            for _ in 0..3 {
                handle_timer(hwnd);
                let mut final_rect = RECT::default();
                GetWindowRect(hwnd, &mut final_rect).unwrap();
                RESIZES.with_borrow_mut(|sizes| {
                    sizes.push((
                        final_rect.right - final_rect.left,
                        final_rect.bottom - final_rect.top,
                    ))
                });
            }
            let _ = DestroyWindow(hwnd);
        }
        let sizes = RESIZES.with_borrow(Clone::clone);
        assert!(
            sizes
                .first()
                .is_some_and(|(width, height)| *width > 0 && *height > 0),
            "test must observe the real layered window update"
        );
        assert!(
            sizes.iter().all(|size| *size == sizes[0]),
            "candidate width oscillates: {sizes:?}"
        );
    }
}
