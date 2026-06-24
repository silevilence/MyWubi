//! Win32 透明分层候选框窗口。
//!
//! 独立线程 + `WS_EX_LAYERED` + `UpdateLayeredWindow` 贴图 + `ArcSwap` 无锁
//! 跨线程读取 + 16ms 定时器轮询。

use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::thread::JoinHandle;

use arc_swap::ArcSwap;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::candidate_data::CandidateData;
use crate::candidate_renderer::{CandidateRenderer, EmbeddedFontProvider};
use crate::registrar;
use crate::screen_geometry::compute_window_rect;

/// 窗口类名（使用 w!() 宏）。
const CLASS_NAME: windows::core::PCWSTR = w!("MyWubiCandidateWindow");

const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 16;
const SHUTDOWN_TIMEOUT_MS: u64 = 500;
const DEFAULT_WIN_W: i32 = 200;
const DEFAULT_WIN_H: i32 = 34;

/// `GWLP_USERDATA` 中嵌入的实例数据。
struct WindowData {
    data_src: std::sync::Arc<ArcSwap<CandidateData>>,
    renderer: OnceLock<CandidateRenderer>,
}

/// 透明候选框窗口句柄（拥有窗口线程生命周期）。
pub struct CandidateWindow {
    join_handle: Option<JoinHandle<()>>,
    hwnd: HWND,
}

impl CandidateWindow {
    /// 在独立线程中启动候选框窗口。
    pub fn spawn(data_src: std::sync::Arc<ArcSwap<CandidateData>>) -> Self {
        let (hwnd_tx, hwnd_rx) = mpsc::channel::<isize>();
        // 安全创建窗口线程——失败时不 panic，避免崩溃宿主进程
        let join_handle = match std::thread::Builder::new()
            .name("MyWubiCandidateThread".into())
            .spawn(move || run_window_thread(data_src, hwnd_tx))
        {
            Ok(h) => Some(h),
            Err(e) => {
                log::error!("候选窗口线程启动失败: {e}");
                return Self { join_handle: None, hwnd: HWND::default() };
            }
        };
        let hwnd = match hwnd_rx.recv() {
            Ok(raw) => HWND(raw as *mut c_void),
            Err(e) => {
                log::error!("候选窗口线程未能发送 HWND: {e}");
                // join_handle 已有效但窗口未创建——join 之
                if let Some(h) = join_handle { let _ = h.join(); }
                return Self { join_handle: None, hwnd: HWND::default() };
            }
        };
        Self { join_handle, hwnd }
    }

    /// 发送退出信号（WM_CLOSE, 最多重试 3 次）并等待线程结束（超时后分离）。
    pub fn shutdown(&mut self) {
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
    fn drop(&mut self) { self.shutdown(); }
}

// ── 窗口线程 ──────────────────────────────────────────────────────

fn run_window_thread(data_src: std::sync::Arc<ArcSwap<CandidateData>>, hwnd_tx: mpsc::Sender<isize>) {
    let hinstance = HINSTANCE(registrar::module_handle() as *mut c_void);
    if hinstance.0.is_null() {
        log::error!("候选窗口: module_handle 未初始化");
        return;
    }

    // 注册窗口类
    let class_name = CLASS_NAME;
    let wc = WNDCLASSW {
        style: WNDCLASS_STYLES(CS_HREDRAW.0 | CS_VREDRAW.0),
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0, cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: HICON::default(), hCursor: HCURSOR::default(),
        hbrBackground: HBRUSH::default(),
        lpszMenuName: windows::core::PCWSTR::null(),
        lpszClassName: class_name,
    };
    if unsafe { RegisterClassW(&wc) } == 0 {
        log::error!("候选窗口: RegisterClassW 失败");
        return;
    }

    // 创建窗口数据
    let window_data = Box::new(WindowData { data_src, renderer: OnceLock::new() });
    let window_data_ptr = Box::into_raw(window_data);

    // 创建分层窗口
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(
                WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0,
            ),
            class_name,
            windows::core::PCWSTR::null(),
            WS_POPUP, 0, 0, 0, 0,
            None, None, Some(hinstance), None,
        )
    } {
        Ok(h) => h,
        Err(e) => {
            log::error!("候选窗口: CreateWindowExW 失败: {e}");
            unsafe { let _ = Box::from_raw(window_data_ptr); }
            return;
        }
    };

    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, window_data_ptr as isize); }
    unsafe { let _ = SetTimer(Some(hwnd), TIMER_ID, TIMER_INTERVAL_MS, None); }

    // 将 HWND 发回给 CandidateWindow，以便 shutdown 时可以发消息。
    let _ = hwnd_tx.send(hwnd.0 as isize);

    // 消息泵（panic 安全）
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut msg = MSG::default();
        unsafe {
            while GetMessageW(&mut msg, Some(hwnd), 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }));
    if let Err(e) = result {
        log::error!("候选窗口消息泵 panic: {:?}", e);
        unsafe { let _ = ShowWindow(hwnd, SW_HIDE); }
    }

    // 清理
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID);
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData;
        if !ptr.is_null() { let _ = Box::from_raw(ptr); }
        let _ = DestroyWindow(hwnd);
    }
}

// ── 窗口过程 ──────────────────────────────────────────────────────

unsafe extern "system" fn wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER if wparam.0 == TIMER_ID => { handle_timer(hwnd); LRESULT(0) }
        WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ── 定时器处理 ────────────────────────────────────────────────────

fn handle_timer(hwnd: HWND) {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData };
    if ptr.is_null() { return; }
    let window_data = unsafe { &*ptr };

    let data = window_data.data_src.load();

    if data.visible {
        // 定位窗口
        if let Some(anchor) = data.anchor {
            let mon_rect = get_monitor_rect(hwnd);
            let win_size = current_window_size(&window_data.renderer);
            let (x, y) = compute_window_rect(anchor, win_size, mon_rect);
            unsafe {
                let _ = SetWindowPos(
                    hwnd, Some(HWND_TOPMOST), x, y, 0, 0,
                    SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0 | SWP_NOSIZE.0),
                );
            }
        }

        // 惰性初始化渲染器（不 panic，避免毒化 OnceLock）
        if window_data.renderer.get().is_none() {
            match CandidateRenderer::new(&EmbeddedFontProvider::new(&[])) {
                Ok(r) => {
                    let _ = window_data.renderer.set(r);
                }
                Err(e) => {
                    log::error!("候选窗口: CandidateRenderer 初始化失败: {e}");
                    // 渲染不可用，降级为隐藏候选框
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                    return;
                }
            }
        }

        // 渲染并贴图
        if let Some(renderer) = window_data.renderer.get() {
            match renderer.render(&data.items, data.highlighted, data.page, data.total_pages, &data.theme) {
                Ok((pixels, width, height)) => {
                    update_layered_window(hwnd, &pixels, width as i32, height as i32);
                }
                Err(e) => log::error!("候选窗口渲染失败: {e}"),
            }
        }
        unsafe { let _ = ShowWindow(hwnd, SW_SHOWNA); }
    } else {
        unsafe { let _ = ShowWindow(hwnd, SW_HIDE); }
    }
}

// ── 辅助函数 ──────────────────────────────────────────────────────

fn get_monitor_rect(hwnd: HWND) -> (i32, i32, i32, i32) {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: mem::size_of::<MONITORINFO>() as u32,
            ..mem::zeroed()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let r = info.rcMonitor;
            (r.left, r.top, r.right, r.bottom)
        } else {
            (0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
        }
    }
}

fn current_window_size(renderer: &OnceLock<CandidateRenderer>) -> (i32, i32) {
    if let Some(r) = renderer.get() {
        let size = r.size();
        (size.width as i32, size.height as i32)
    } else {
        (DEFAULT_WIN_W, DEFAULT_WIN_H)
    }
}

// ── UpdateLayeredWindow 贴图 ──────────────────────────────────────

/// RGBA8 → BGRA 转换后通过 `UpdateLayeredWindow` 贴图。
fn update_layered_window(hwnd: HWND, pixels: &[u8], width: i32, height: i32) {
    if width <= 0 || height <= 0 || pixels.len() < (width * height * 4) as usize {
        return;
    }
    unsafe {
        let hdc_screen = GetDC(Some(HWND::default()));
        if hdc_screen.is_invalid() { return; }

        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() { let _ = ReleaseDC(None, hdc_screen); return; }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width, biHeight: -height,
                biPlanes: 1, biBitCount: 32,
                biCompression: BI_RGB.0,
                ..mem::zeroed()
            },
            bmiColors: [RGBQUAD::default(); 1],
        };

        let mut bits: *mut c_void = ptr::null_mut();
        let hbitmap = match CreateDIBSection(
            Some(hdc_mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0,
        ) {
            Ok(b) => b,
            Err(e) => {
                log::error!("候选窗口: CreateDIBSection 失败: {e}");
                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(None, hdc_screen);
                return;
            }
        };
        if hbitmap.is_invalid() || bits.is_null() {
            // 不在无效句柄上调用 DeleteObject（未定义行为）
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return;
        }

        let old_bitmap = SelectObject(hdc_mem, HGDIOBJ(hbitmap.0));

        // RGBA → BGRA 转换
        let pixel_count = (width * height) as usize;
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, pixel_count * 4);
        for i in 0..pixel_count {
            dst[i * 4]     = pixels[i * 4 + 2]; // B ← R
            dst[i * 4 + 1] = pixels[i * 4 + 1]; // G ← G
            dst[i * 4 + 2] = pixels[i * 4];     // R ← B
            dst[i * 4 + 3] = pixels[i * 4 + 3]; // A ← A
        }

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let pt_src = POINT { x: 0, y: 0 };
        let size = SIZE { cx: width, cy: height };

        let _ = UpdateLayeredWindow(
            hwnd, Some(hdc_screen), None, Some(&size),
            Some(hdc_mem), Some(&pt_src),
            COLORREF::default(), Some(&blend), ULW_ALPHA,
        );

        SelectObject(hdc_mem, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(hbitmap.0));
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);
    }
}
