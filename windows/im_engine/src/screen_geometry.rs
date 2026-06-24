// windows/im_engine/src/screen_geometry.rs

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::TextServices::{ITfContext, ITfContextView, ITfRange, TF_SELECTION};
use crate::candidate_data::ScreenPoint;

/// TSF 默认编辑会话 Cookie。
const TF_DEFAULT_SELECTION: u32 = 0;

/// 屏幕边缘 padding（像素）。
const EDGE_PADDING: i32 = 8;

/// 从 TSF ITfContext 获取光标屏幕坐标。
///
/// 步骤：
/// 1. ITfContext::GetSelection 获取当前退化选区（即光标位置）；
/// 2. ITfContext::GetActiveView 获取 ITfContextView；
/// 3. ITfContextView::GetTextExt 获取选区文本坐标矩形；
/// 4. ITfContextView::GetWnd 获取文档窗口 HWND；
/// 5. ClientToScreen 将文本坐标转换为屏幕绝对坐标。
pub fn get_caret_position(context: &ITfContext) -> Option<ScreenPoint> {
    // 1. 获取当前选区（退化选区 == 光标位置）
    let mut selection = [unsafe { std::mem::zeroed::<TF_SELECTION>() }];
    let mut fetched: u32 = 0;
    unsafe {
        context
            .GetSelection(TF_DEFAULT_SELECTION, 0, &mut selection, &mut fetched)
            .ok()?;
    }
    if fetched == 0 {
        return None;
    }
    let sel = &selection[0];
    // TF_SELECTION.range 是 ManuallyDrop<ITfRange>，&* 安全解引用。
    // 若 windows-rs 版本变更导致布局不同，此处需调整。

    // 2. 获取活动视图
    let view: ITfContextView = unsafe { context.GetActiveView().ok()? };

    // 3. 获取选区 bounding rect（文本坐标）
    let mut rect = RECT::default();
    let mut clipped = BOOL::default();
    let range: &ITfRange = (&*sel.range).as_ref()?;
    unsafe {
        view.GetTextExt(TF_DEFAULT_SELECTION, range, &mut rect, &mut clipped)
            .ok()?;
    }

    // 4. 获取文档窗口 HWND
    let hwnd: HWND = unsafe { view.GetWnd().ok()? };

    // 5. 转换到屏幕坐标（取光标左下角）
    let mut pt = POINT {
        x: rect.left,
        y: rect.bottom,
    };
    unsafe {
        let _ = ClientToScreen(hwnd, &mut pt);
    }

    Some(ScreenPoint { x: pt.x, y: pt.y })
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

    // 垂直避让：光标在屏幕下半部 → 候选框翻到光标上方
    let anchor_mid_y = anchor.y + (win_h / 2);
    if anchor_mid_y > (mon_top + mon_bottom) / 2 {
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
