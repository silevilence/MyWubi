//! Win32 透明分层候选框窗口。
//!
//! 独立线程 + `WS_EX_LAYERED` + `UpdateLayeredWindow` 贴图 + `ArcSwap` 无锁
//! 跨线程读取 + 16ms 定时器轮询。

use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::mpsc;
use std::thread::JoinHandle;

use arc_swap::ArcSwap;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::candidate_data::CandidateData;
use crate::screen_geometry::compute_window_rect;

/// 窗口类名（使用 w!() 宏）。
const CLASS_NAME: windows::core::PCWSTR = w!("MyWubiCandidateWindow");

const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 16;
const SHUTDOWN_TIMEOUT_MS: u64 = 500;
const DEFAULT_WIN_W: i32 = 200;
const DEFAULT_WIN_H: i32 = 34;
const WINDOW_BORDER_COLOR: u32 = 0xFFD9DEE5;

/// `GWLP_USERDATA` 中嵌入的实例数据。
struct WindowData {
    data_src: std::sync::Arc<ArcSwap<CandidateData>>,
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
    let hinstance = HINSTANCE(crate::module_handle() as *mut c_void);
    if hinstance.0.is_null() {
        log::error!("候选窗口: module_handle 未初始化");
        return;
    }

    // 窗口类只需注册一次（整个进程生命周期内有效）。
    // TSF 在同一进程中可能多次 Activate/Deactivate，重复注册会失败。
    static CLASS_REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static REGISTER_ONCE: std::sync::Once = std::sync::Once::new();
    REGISTER_ONCE.call_once(|| {
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
    let window_data = Box::new(WindowData { data_src });
    let window_data_ptr = Box::into_raw(window_data);

    // 创建分层窗口
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(
                WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0,
            ),
            CLASS_NAME,
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

    // 注意：class_name 变量在 Once 闭包内作用域，此处使用静态常量 CLASS_NAME。
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, window_data_ptr as isize); }
    unsafe { let _ = SetTimer(Some(hwnd), TIMER_ID, TIMER_INTERVAL_MS, None); }

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
        // 计算窗口尺寸
        let (win_w, win_h) = measure_candidate_window_size(hwnd, &data);

        // 定位窗口
        let mon_rect = get_monitor_rect(hwnd);
        let (pos_x, pos_y) = if let Some(anchor) = data.anchor {
            compute_window_rect(anchor, (win_w, win_h), mon_rect)
        } else {
            // 无锚点时用鼠标位置回退，再不行用显示器中心
            let fallback = crate::screen_geometry::get_cursor_position()
                .map(|p| compute_window_rect(p, (win_w, win_h), mon_rect))
                .unwrap_or((
                    (mon_rect.2 - mon_rect.0 - win_w) / 2,
                    (mon_rect.3 - mon_rect.1 - win_h) / 2,
                ));
            fallback
        };
        unsafe {
            let _ = SetWindowPos(
                hwnd, Some(HWND_TOPMOST), pos_x, pos_y, win_w, win_h,
                SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0),
            );
        }

        // 使用 GDI 渲染候选框并贴图
        gdi_render_candidate_window(hwnd, window_data, &data);

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

/// 根据候选数据和编码计算窗口尺寸（双行显示：编码行 + 候选行）。
fn measure_candidate_window_size(_hwnd: HWND, data: &CandidateData) -> (i32, i32) {
    if data.items.is_empty() && data.spelling.is_empty() {
        return (DEFAULT_WIN_W, DEFAULT_WIN_H);
    }
    let fs_px = crate::screen_geometry::pt_to_px(data.theme.font_size).max(12);
    if data.items.is_empty() {
        let approx_w = (data.spelling.len() as i32) * fs_px + 24;
        return (approx_w.max(60).min(800), fs_px + 20);
    }
    let total_w = data
        .items
        .iter()
        .map(|item| (item.label.chars().count() + item.text.chars().count() + item.hint.chars().count()) as i32 * fs_px / 2 + 16)
        .sum::<i32>()
        + 20;
    let show_spelling = !data.spelling.is_empty();
    let row_h = (fs_px as f64 * 1.4) as i32;
    let total_h = if show_spelling { row_h * 2 + 6 } else { row_h + 6 };
    (total_w.min(800), total_h.max(DEFAULT_WIN_H))
}

// ── GDI 渲染 ──────────────────────────────────────────────────────

/// 分解 ARGB 0xAARRGGBB 为 (r, g, b, a) 分量。
fn unpack_argb(color: u32) -> (u8, u8, u8, u8) {
    let a = ((color >> 24) & 0xFF) as u8;
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    (r, g, b, a)
}

/// 把 ARGB 颜色转换为 GDI COLORREF（0x00BBGGRR）。
fn argb_to_colorref(color: u32) -> COLORREF {
    let (r, g, b, _) = unpack_argb(color);
    COLORREF((b as u32) | ((g as u32) << 8) | ((r as u32) << 16))
}

fn fill_alpha_channel(pixels: &mut [u8], alpha: u8) {
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[3] = alpha;
    }
}

/// 使用 GDI 渲染候选框（两行：编码行 + 候选词行）并通过 UpdateLayeredWindow 贴图。
fn gdi_render_candidate_window(
    hwnd: HWND,
    _window_data: &WindowData,
    data: &CandidateData,
) {
    if data.items.is_empty() && data.spelling.is_empty() {
        return;
    }

    unsafe {
        let hdc_screen = GetDC(Some(HWND::default()));
        if hdc_screen.is_invalid() { return; }
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() { let _ = ReleaseDC(None, hdc_screen); return; }

        // font_size 以 pt 为单位，转换为像素
        let fs_px = crate::screen_geometry::pt_to_px(data.theme.font_size).max(12);
        let row_h = (fs_px as f64 * 1.4) as i32;
        let padding = 8i32;
        let item_gap = 8i32;
        let show_spelling = !data.spelling.is_empty();
        let total_h = if show_spelling { row_h * 2 + 6 } else { row_h + 6 };

        let font = CreateFontW(
            -fs_px, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY, (DEFAULT_PITCH.0 | FF_DONTCARE.0).into(),
            w!("Microsoft YaHei"),
        );
        if font.is_invalid() {
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return;
        }
        let old_font = SelectObject(hdc_mem, HGDIOBJ(font.0));
        let _ = SetBkMode(hdc_mem, TRANSPARENT);

        // 计算窗口宽度
        let mut item_widths = Vec::with_capacity(data.items.len());
        for item in &data.items {
            let lw: Vec<u16> = item.label.encode_utf16().collect();
            let tw: Vec<u16> = item.text.encode_utf16().collect();
            let hw: Vec<u16> = item.hint.encode_utf16().collect();
            let mut ls = SIZE::default();
            let mut ts = SIZE::default();
            let mut hs = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc_mem, &lw, &mut ls);
            let _ = GetTextExtentPoint32W(hdc_mem, &tw, &mut ts);
            let _ = GetTextExtentPoint32W(hdc_mem, &hw, &mut hs);
            item_widths.push((ls.cx + ts.cx + hs.cx + 4).max(24));
        }
        let total_cand_w = item_widths.iter().sum::<i32>()
            + item_gap * data.items.len().saturating_sub(1) as i32
            + padding * 2;
        let mut spell_w = 0i32;
        if show_spelling {
            let sw: Vec<u16> = data.spelling.encode_utf16().collect();
            let mut sz = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc_mem, &sw, &mut sz);
            spell_w = sz.cx + padding * 2;
        }
        let win_w = total_cand_w.max(spell_w).max(DEFAULT_WIN_W);
        let win_h = total_h.max(DEFAULT_WIN_H);

        // 创建 DIB
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: win_w, biHeight: -win_h,
                biPlanes: 1, biBitCount: 32,
                biCompression: BI_RGB.0,
                ..mem::zeroed()
            },
            bmiColors: [RGBQUAD::default(); 1],
        };
        let mut bits: *mut c_void = ptr::null_mut();
        let hbitmap = match CreateDIBSection(Some(hdc_mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) => b,
            Err(e) => {
                log::error!("候选窗口: CreateDIBSection 失败: {e}");
                let _ = DeleteDC(hdc_mem);
                let _ = ReleaseDC(None, hdc_screen);
                return;
            }
        };
        if hbitmap.is_invalid() || bits.is_null() {
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return;
        }
        let old_bitmap = SelectObject(hdc_mem, HGDIOBJ(hbitmap.0));
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (win_w * win_h * 4) as usize);

        // 填充背景
        let bg = CreateSolidBrush(argb_to_colorref(data.theme.background_color));
        let _ = FillRect(hdc_mem, &RECT { left: 0, top: 0, right: win_w, bottom: win_h }, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));

        let border = CreateSolidBrush(argb_to_colorref(WINDOW_BORDER_COLOR));
        let _ = FrameRect(hdc_mem, &RECT { left: 0, top: 0, right: win_w, bottom: win_h }, border);
        let _ = DeleteObject(HGDIOBJ(border.0));

        // ── 绘制编码行（第一行）──
        if show_spelling {
            let sw: Vec<u16> = data.spelling.encode_utf16().collect();
            let mut sz = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc_mem, &sw, &mut sz);
            let _ = SetTextColor(hdc_mem, argb_to_colorref(data.theme.primary_color));
            let _ = TextOutW(hdc_mem, padding, (row_h - sz.cy) / 2, &sw);
        }

        // ── 绘制候选词行（第二行）──
        let cand_y = if show_spelling { row_h + 2 } else { 0 };
        let mut cx = padding;
        for (i, item) in data.items.iter().enumerate() {
            let lw: Vec<u16> = item.label.encode_utf16().collect();
            let tw: Vec<u16> = item.text.encode_utf16().collect();
            let hw: Vec<u16> = item.hint.encode_utf16().collect();
            let mut ls = SIZE::default();
            let mut ts = SIZE::default();
            let mut hs = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc_mem, &lw, &mut ls);
            let _ = GetTextExtentPoint32W(hdc_mem, &tw, &mut ts);
            let _ = GetTextExtentPoint32W(hdc_mem, &hw, &mut hs);
            let item_w = item_widths.get(i).copied().unwrap_or(ls.cx + ts.cx + hs.cx + 4);
            if i == data.highlighted {
                let hl = CreateSolidBrush(argb_to_colorref(data.theme.highlight_color));
                let _ = FillRect(hdc_mem, &RECT {
                    left: cx - 2, top: cand_y + 1,
                    right: cx + item_w + 2, bottom: cand_y + row_h - 1,
                }, hl);
                let _ = DeleteObject(HGDIOBJ(hl.0));
                let _ = SetTextColor(hdc_mem, COLORREF(0x00FFFFFF));
                let _ = TextOutW(hdc_mem, cx, cand_y + (row_h - ls.cy) / 2, &lw);
                let _ = TextOutW(hdc_mem, cx + ls.cx, cand_y + (row_h - ts.cy) / 2, &tw);
                let _ = TextOutW(hdc_mem, cx + ls.cx + ts.cx, cand_y + (row_h - hs.cy) / 2, &hw);
            } else {
                let _ = SetTextColor(hdc_mem, argb_to_colorref(data.theme.primary_color));
                let _ = TextOutW(hdc_mem, cx, cand_y + (row_h - ls.cy) / 2, &lw);
                let _ = SetTextColor(hdc_mem, COLORREF(0x00000000));
                let _ = TextOutW(hdc_mem, cx + ls.cx, cand_y + (row_h - ts.cy) / 2, &tw);
                let _ = SetTextColor(hdc_mem, argb_to_colorref(data.theme.primary_color));
                let _ = TextOutW(hdc_mem, cx + ls.cx + ts.cx, cand_y + (row_h - hs.cy) / 2, &hw);
            }
            cx += item_w + item_gap;
        }

        // 分层窗口上的纯黑文本也必须带 alpha，否则会被当成完全透明。
        fill_alpha_channel(pixels, 255);

        // UpdateLayeredWindow
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0, SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = UpdateLayeredWindow(
            hwnd, Some(hdc_screen), None, Some(&SIZE { cx: win_w, cy: win_h }),
            Some(hdc_mem), Some(&POINT { x: 0, y: 0 }),
            COLORREF::default(), Some(&blend), ULW_ALPHA,
        );

        let _ = SelectObject(hdc_mem, old_font);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let _ = SelectObject(hdc_mem, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(hbitmap.0));
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_alpha_fix_keeps_black_text_visible() {
        let mut pixels = vec![0u8, 0u8, 0u8, 0u8, 255u8, 255u8, 255u8, 0u8];

        fill_alpha_channel(&mut pixels, 255);

        assert_eq!(pixels[3], 255);
        assert_eq!(pixels[7], 255);
    }
}
