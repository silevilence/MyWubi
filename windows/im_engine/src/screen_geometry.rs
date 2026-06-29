// windows/im_engine/src/screen_geometry.rs

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetDC, GetDeviceCaps, ReleaseDC, LOGPIXELSY};
use windows::Win32::UI::TextServices::{ITfContext, ITfContextView, ITfRange};
use windows::Win32::UI::WindowsAndMessaging::{GetCaretPos, GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GUITHREADINFO};
use crate::candidate_data::ScreenPoint;

/// 屏幕边缘 padding（像素）。
const EDGE_PADDING: i32 = 8;
const DEFAULT_DPI: i32 = 96;

/// 使用带有效 edit cookie 的 TSF range 获取屏幕锚点。
pub fn get_range_position(context: &ITfContext, ec: u32, range: &ITfRange) -> Option<ScreenPoint> {
    let view: ITfContextView = unsafe { context.GetActiveView().ok()? };
    let mut rect = RECT::default();
    let mut clipped = BOOL::default();
    unsafe {
        view.GetTextExt(ec, range, &mut rect, &mut clipped)
            .ok()?;
    }

    // TSF 文档窗口最小化/不可见时会返回 {0,0,0,0}。
    if rect.left == 0 && rect.top == 0 && rect.right == 0 && rect.bottom == 0 {
        return None;
    }

    let _ = clipped;
    Some(ScreenPoint { x: rect.left, y: rect.bottom })
}

fn get_window_dpi(hwnd: HWND) -> i32 {
    unsafe {
        let (dc_target, release_target) = if hwnd.is_invalid() || hwnd.0.is_null() {
            (HWND::default(), None)
        } else {
            (hwnd, Some(hwnd))
        };
        let hdc = GetDC(Some(dc_target));
        if hdc.is_invalid() {
            return DEFAULT_DPI;
        }
        let dpi = GetDeviceCaps(Some(hdc), LOGPIXELSY);
        let _ = ReleaseDC(release_target, hdc);
        if dpi > 0 { dpi } else { DEFAULT_DPI }
    }
}

pub fn pt_to_px_for_dpi(pt: u16, dpi: i32) -> i32 {
    ((pt as i32) * dpi + 36) / 72
}

pub fn pt_to_px_for_window(hwnd: HWND, pt: u16) -> i32 {
    pt_to_px_for_dpi(pt, get_window_dpi(hwnd))
}

/// 将 pt（点）转换为像素（基于屏幕 DPI）。
pub fn pt_to_px(pt: u16) -> i32 {
    pt_to_px_for_dpi(pt, get_window_dpi(HWND::default()))
}

/// 将 pt（点）转换为大致行高（像素），含 ~1.3 倍行距。
pub fn pt_to_line_height(pt: u16) -> i32 {
    (pt_to_px(pt) as f64 * 1.3) as i32
}

pub fn pt_to_line_height_for_window(hwnd: HWND, pt: u16) -> i32 {
    (pt_to_px_for_window(hwnd, pt) as f64 * 1.3) as i32
}

/// 使用 Win32 GetCaretPos 获取当前窗口光标屏幕坐标（无需 TSF edit cookie）。
///
/// `font_size_pt` 用于计算候选框在文字行下方的垂直偏移量。
///
/// 使用 `GetGUIThreadInfo` 获取拥有光标的子窗口句柄，确保 `ClientToScreen`
/// 使用正确的窗口（资源管理器地址栏、记事本等均为子窗口，非顶层窗口）。
pub fn get_caret_position_win32(font_size_pt: u16) -> Option<ScreenPoint> {
    unsafe {
        // 获取当前线程 GUI 信息，获取拥有光标的子窗口句柄
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(0, &mut info).is_err() {
            return None;
        }
        let caret_hwnd = info.hwndCaret;
        if caret_hwnd.is_invalid() || caret_hwnd.0.is_null() {
            // 回退：使用 GetForegroundWindow
            let fg = GetForegroundWindow();
            if fg.is_invalid() || fg.0.is_null() {
                return None;
            }
            let mut pt = POINT::default();
            if GetCaretPos(&mut pt).is_err() {
                return None;
            }
            let _ = ClientToScreen(fg, &mut pt);
            let line_h = pt_to_line_height_for_window(fg, font_size_pt);
            return Some(ScreenPoint { x: pt.x, y: pt.y + line_h + 2 });
        }
        let mut pt = POINT::default();
        if GetCaretPos(&mut pt).is_err() {
            return None;
        }
        // 使用拥有光标的子窗口进行坐标转换（而非 GetForegroundWindow）
        let _ = ClientToScreen(caret_hwnd, &mut pt);
        // GetCaretPos 返回的 y 是文字基线 (baseline) 坐标。
        // 候选框应位于文字行下方：基线 y + 行高 + 2px 间距
        let line_h = pt_to_line_height_for_window(caret_hwnd, font_size_pt);
        Some(ScreenPoint { x: pt.x, y: pt.y + line_h + 2 })
    }
}

/// 使用 Win32 GetCursorPos 获取鼠标位置作为锚点回退。
pub fn get_cursor_position() -> Option<ScreenPoint> {
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return None;
        }
        Some(ScreenPoint { x: pt.x, y: pt.y + 20 })
    }
}

/// 计算候选框窗口的左上角屏幕坐标，自动避让屏幕边缘。
///
/// # Arguments
/// * `anchor` — 光标左下角的屏幕坐标。
/// * `window_size` — 候选框窗口的 (width, height)。
/// * `monitor_rect` — 当前显示器的 (left, top, right, bottom)。
///
/// # Returns
/// 候选框窗口左上角的 (x, y) 坐标。
pub fn compute_window_rect(
    anchor: ScreenPoint,
    window_size: (i32, i32),
    monitor_rect: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (win_w, win_h) = window_size;
    let (mon_left, mon_top, mon_right, mon_bottom) = monitor_rect;

    // 默认：候选框左上角对齐光标左下角（即锚点本身）
    let mut x = anchor.x;
    let mut y = anchor.y;

    // 垂直避让：光标在屏幕下方 ~2/3 区域时 → 候选框翻到光标上方
    // 避免在屏幕中心区域就翻上去，默认始终在光标下方显示
    let flip_threshold = (mon_top + mon_bottom * 2) / 3;
    if anchor.y > flip_threshold {
        y = anchor.y - win_h;
    }

    // 底部边界
    if y + win_h > mon_bottom - EDGE_PADDING {
        y = mon_bottom - win_h - EDGE_PADDING;
    }
    // 顶部边界
    if y < mon_top + EDGE_PADDING {
        y = mon_top + EDGE_PADDING;
    }

    // 右侧避让：候选框超出右边缘则向左偏移
    if x + win_w > mon_right - EDGE_PADDING {
        x = mon_right - win_w - EDGE_PADDING;
    }
    // 左侧边界
    if x < mon_left + EDGE_PADDING {
        x = mon_left + EDGE_PADDING;
    }

    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pt_to_px_uses_explicit_dpi() {
        assert_eq!(pt_to_px_for_dpi(14, 96), 19);
        assert_eq!(pt_to_px_for_dpi(14, 144), 28);
    }

    /// 光标在屏幕中央 → 候选框正常出现在光标下方。
    #[test]
    fn normal_below_cursor() {
        let anchor = ScreenPoint { x: 500, y: 400 };
        let (x, y) = compute_window_rect(anchor, (200, 34), (0, 0, 1920, 1080));
        assert_eq!((x, y), (500, 400));
    }

    /// 光标在屏幕底部 → 候选框翻到上方。
    #[test]
    fn flip_above_when_near_bottom() {
        let anchor = ScreenPoint { x: 500, y: 1000 };
        let (x, y) = compute_window_rect(anchor, (200, 34), (0, 0, 1920, 1080));
        // y = 1000 - 34 = 966
        assert_eq!(x, 500);
        assert!(y < 1000, "候选框应在光标上方");
    }

    /// 候选框超出右边缘 → 向左贴边。
    #[test]
    fn clamp_right_edge() {
        let anchor = ScreenPoint { x: 1850, y: 400 };
        let (x, y) = compute_window_rect(anchor, (200, 34), (0, 0, 1920, 1080));
        // 1850 + 200 = 2050 > 1920 - 8 = 1912
        // x = 1920 - 200 - 8 = 1712
        assert_eq!(x, 1712);
        assert_eq!(y, 400);
    }

    /// 候选框超出左边缘 → 向右贴边。
    #[test]
    fn clamp_left_edge() {
        let anchor = ScreenPoint { x: -10, y: 400 };
        let (x, y) = compute_window_rect(anchor, (200, 34), (0, 0, 1920, 1080));
        assert_eq!(x, 8);
        assert_eq!(y, 400);
    }
}
