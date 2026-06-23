// windows/im_engine/src/screen_geometry.rs

use windows::Win32::UI::TextServices::ITfContext;
use crate::candidate_data::ScreenPoint;

/// 屏幕边缘 padding（像素）。
const EDGE_PADDING: i32 = 8;

/// 从 TSF ITfContext 获取光标屏幕坐标。
///
/// 通过 ITfContext::GetStatus 获取文本服务状态中的光标位置，
/// 再翻译为屏幕绝对坐标。
pub fn get_caret_position(_context: &ITfContext) -> Option<ScreenPoint> {
    // FIXME: 实现通过 ITfContext::GetStatus + ITfContextView::GetTextExt
    // 获取精确光标矩形，当前返回占位值供渲染联调。
    None
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
