//! Win32 `ChooseColor` 原生颜色对话框封装。
//!
//! 返回 ARGB，Alpha 通道默认 0xFF（配置中的 ARGB 高 8 位）。

#[cfg(windows)]
pub fn pick_color(initial_rgb: u32) -> Option<u32> {
    use windows::Win32::UI::Controls::Dialogs::{
        ChooseColorW, CHOOSECOLORW, CC_FULLOPEN, CC_RGBINIT,
    };
    use windows::Win32::Foundation::{COLORREF, HWND};

    let mut custom_colors = [COLORREF(0); 16];
    let mut cc = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: HWND::default(),
        rgbResult: COLORREF(initial_rgb & 0x00FFFFFF),
        lpCustColors: custom_colors.as_mut_ptr(),
        Flags: CC_RGBINIT | CC_FULLOPEN,
        ..Default::default()
    };

    unsafe {
        if ChooseColorW(&mut cc).as_bool() {
            Some(0xFF000000 | (cc.rgbResult.0 & 0x00FFFFFF))
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub fn pick_color(_initial_rgb: u32) -> Option<u32> {
    None
}