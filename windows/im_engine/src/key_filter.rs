//! 按键事件映射：将 Windows 虚拟键码 / LPARAM 转换为 [`core_engine::InputEvent`]。
//!
//! [ITfKeyEventSink::OnKeyDown] 回调中传入 `wparam` 即虚拟键码 (VK_*),
//! `lparam` 包含重复次数、扫描码及扩展键标志位。本模块负责把常见的输入法相关
//! 按键翻译为跨平台 [`InputEvent`]，其余返回 `None`（视为 passthrough）。
//!
//! 翻译规则：
//! * 功能键（Space / Enter / Backspace / Esc）→ 对应 InputEvent；
//! * 翻页键 → 由配置动态映射；
//! * 其他可打印字符 → 通过 `ToUnicode` API 由系统根据当前键盘布局和修饰键状态
//!   返回实际输入字符（支持 AZERTY、Dvorak 等任意布局），避免硬编码映射表；

use core_engine::{config::Hotkey, InputEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

/// 检测系统修饰键（Ctrl / Alt / Win）是否处于按下状态。
///
/// 当这些修饰键被按下时，当前按键属于系统级组合键（如 Ctrl+C、Alt+Tab），
/// 输入法应当直接放行而不拦截。Shift 不在此列——Shift+字母 仍是合法的中文
/// 编码输入场景。
///
/// 使用 `GetKeyState` 读取消息队列中的按键状态，其高位 (bit 15) 为 1 表示
/// 该键当前被按下。TSF 回调在消息泵内同步执行，因此该 API 可靠。
pub fn is_system_modifier_pressed() -> bool {
    unsafe {
        (GetKeyState(VK_CONTROL.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_LCONTROL.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_RCONTROL.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_MENU.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_LMENU.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_RMENU.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_LWIN.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_RWIN.0 as i32) & 0x8000u16 as i16) != 0
    }
}

/// 检测 Shift 键是否处于按下状态（仅 Shift，不包括 Ctrl/Alt/Win）。
pub fn is_shift_pressed() -> bool {
    unsafe {
        (GetKeyState(VK_SHIFT.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_LSHIFT.0 as i32) & 0x8000u16 as i16) != 0
            || (GetKeyState(VK_RSHIFT.0 as i32) & 0x8000u16 as i16) != 0
    }
}

/// 配置中翻页键的字符串标识 → 虚拟键码映射。
fn key_name_to_vk(name: &str) -> Option<u16> {
    match name {
        "comma" => Some(VK_OEM_COMMA.0),
        "period" => Some(VK_OEM_PERIOD.0),
        "semicolon" => Some(VK_OEM_1.0),
        "quote" => Some(VK_OEM_7.0),
        "minus" => Some(VK_OEM_MINUS.0),
        "equal" => Some(VK_OEM_PLUS.0),
        "space" => Some(VK_SPACE.0),
        "left" => Some(VK_LEFT.0),
        "right" => Some(VK_RIGHT.0),
        "page_up" => Some(VK_PRIOR.0),
        "page_down" => Some(VK_NEXT.0),
        _ => None,
    }
}

/// 通过 `ToUnicode` API 查询虚拟键码在当前键盘布局下实际产生的字符。
///
/// 先调用 `GetKeyboardState` 获取当前完整键盘状态（含 Shift / CapsLock 等），
/// 再传给 `ToUnicode` 让它结合当前键盘布局 (HKL) 计算实际输出字符。
/// 支持 AZERTY / Dvorak 等任意布局。
///
/// 返回 `None` 表示该按键不产生可打印字符（如功能键、组合键等）。
fn to_char(vk: u16, lparam: isize) -> Option<char> {
    // 读取当前完整键盘状态（256 字节数组，每字节对应一个 VK，高位=按下）
    let mut key_state = [0u8; 256];
    unsafe {
        let _ = GetKeyboardState(&mut key_state);
    };

    // 清除 Ctrl/Alt/Win 的状态位，避免 ToUnicode 把它们当作 AltGr（
    // 某些键盘布局上 AltGr 会合成特殊字符，我们只关心 Shift/CapsLock）。
    key_state[VK_CONTROL.0 as usize] = 0;
    key_state[VK_LCONTROL.0 as usize] = 0;
    key_state[VK_RCONTROL.0 as usize] = 0;
    key_state[VK_MENU.0 as usize] = 0; // Alt
    key_state[VK_LMENU.0 as usize] = 0;
    key_state[VK_RMENU.0 as usize] = 0;
    key_state[VK_LWIN.0 as usize] = 0;
    key_state[VK_RWIN.0 as usize] = 0;

    // LPARAM 的 bit 16..23 为扫描码
    let scan_code = ((lparam >> 16) & 0xFF) as u32;
    // LPARAM 的 bit 24 为扩展键标志
    let extended = ((lparam >> 24) & 1) != 0;
    // 构造 ToUnicode 的扫描码参数：扩展键需设置 bit 24
    let sc = if extended {
        scan_code | 0x0100_0000
    } else {
        scan_code
    };

    let mut buf = [0u16; 4];
    let n = unsafe { ToUnicode(vk as u32, sc, Some(&key_state), &mut buf, 0) };
    match n {
        1 => char::decode_utf16(buf[..1].iter().copied())
            .next()
            .and_then(|r| r.ok()),
        2 if buf[0] >= 0xD800 && buf[0] < 0xDC00 => {
            // 代理对：罕见的 Supplementary Plane 字符
            char::decode_utf16(buf[..2].iter().copied())
                .next()
                .and_then(|r| r.ok())
        }
        _ => None,
    }
}

fn classify_printable_char(c: char, _shift_pressed: bool) -> InputEvent {
    ascii_to_chinese_punctuation(c).map_or(InputEvent::Char(c), InputEvent::Symbol)
}

fn ascii_to_chinese_punctuation(c: char) -> Option<char> {
    match c {
        ',' => Some('，'),
        '.' => Some('。'),
        '?' => Some('？'),
        '!' => Some('！'),
        ';' => Some('；'),
        ':' => Some('：'),
        '\'' => Some('’'),
        '"' => Some('”'),
        '(' => Some('（'),
        ')' => Some('）'),
        '[' => Some('【'),
        ']' => Some('】'),
        '<' => Some('《'),
        '>' => Some('》'),
        '/' | '\\' => Some('、'),
        _ => None,
    }
}

/// 把 (`wparam`, `lparam`) 解析为通用 [`InputEvent`]；返回 `None` 表示该按键与本输入法无关。
///
/// `wparam` 为虚拟键码；`lparam` 包含扫描码与扩展键标志，传给 `ToUnicode`
/// 以准确获取当前键盘布局下的实际字符。
pub fn translate(
    wparam: usize,
    lparam: isize,
    is_selecting: bool,
    hotkey: &Hotkey,
) -> Option<InputEvent> {
    let vk = wparam as u16;
    let shift_pressed = is_shift_pressed();

    // 候选态下的快速选词优先于翻页（仅 Shift 未按下时）。
    if !shift_pressed && is_selecting {
        if let Some(select_second_vk) = key_name_to_vk(&hotkey.select_second) {
            if vk == select_second_vk {
                return Some(InputEvent::Select(2));
            }
        }
        if let Some(select_third_vk) = key_name_to_vk(&hotkey.select_third) {
            if vk == select_third_vk {
                return Some(InputEvent::Select(3));
            }
        }
    }

    // 翻页键优先匹配（仅候选态且 Shift 未按下时）
    if !shift_pressed && is_selecting {
        if let Some(pn_vk) = key_name_to_vk(&hotkey.page_next) {
            if vk == pn_vk {
                return Some(InputEvent::PageNext);
            }
        }
        if let Some(pp_vk) = key_name_to_vk(&hotkey.page_prev) {
            if vk == pp_vk {
                return Some(InputEvent::PagePrev);
            }
        }
    }

    // 功能键：不依赖 ToUnicode，直接映射
    match vk {
        v if v == VK_SPACE.0 => return Some(InputEvent::Space),
        v if v == VK_RETURN.0 => return Some(InputEvent::Enter),
        v if v == VK_BACK.0 => return Some(InputEvent::Backspace),
        v if v == VK_ESCAPE.0 => return Some(InputEvent::Esc),
        _ => {}
    }

    // 可打印字符：由系统根据键盘布局 + 修饰键状态自动映射。
    to_char(vk, lparam).map(|c| classify_printable_char(c, shift_pressed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_OEM_1, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PLUS,
    };

    fn default_hotkey() -> Hotkey {
        Hotkey::default()
    }

    fn translate_idle(wparam: usize, lparam: isize) -> Option<InputEvent> {
        translate(wparam, lparam, false, &default_hotkey())
    }

    #[test]
    fn translate_space_and_enter() {
        assert_eq!(
            translate_idle(VK_SPACE.0 as usize, 0),
            Some(InputEvent::Space)
        );
        assert_eq!(
            translate_idle(VK_RETURN.0 as usize, 0),
            Some(InputEvent::Enter)
        );
    }

    #[test]
    fn translate_backspace_and_esc() {
        assert_eq!(
            translate_idle(VK_BACK.0 as usize, 0),
            Some(InputEvent::Backspace)
        );
        assert_eq!(
            translate_idle(VK_ESCAPE.0 as usize, 0),
            Some(InputEvent::Esc)
        );
    }

    /// 以下 ToUnicode 相关测试依赖系统键盘布局为英文（QWERTY）。
    /// 在 CI 或不同机器上可能因布局差异导致 ToUnicode 返回不同结果，
    /// 因此这些测试仅验证基本功能键映射和 `key_name_to_vk`，不硬编码可打印字符。

    /// `to_char` 的结果依赖运行时键盘状态和布局，不适合单元测试。
    /// 功能键（Space/Enter/Backspace/Esc）在 `translate()` 中已被提前拦截，
    /// 不会走到 `to_char`，无需在测试中验证。

    #[test]
    fn translate_unknown_returns_none() {
        // 虚拟键码 0xFF 通常不产生任何字符
        assert_eq!(translate_idle(0xFF as usize, 0), None);
    }

    #[test]
    fn translate_common_punctuation_hotkeys_when_selecting() {
        // 默认配置下逗号/句号在候选态映射为翻页键（无 Shift）
        assert_eq!(
            translate(VK_OEM_COMMA.0 as usize, 0, true, &default_hotkey()),
            Some(InputEvent::PageNext)
        );
        assert_eq!(
            translate(VK_OEM_PERIOD.0 as usize, 0, true, &default_hotkey()),
            Some(InputEvent::PagePrev)
        );
    }

    #[test]
    fn page_next_takes_priority_over_page_prev() {
        let mut hotkey = default_hotkey();
        hotkey.page_prev = hotkey.page_next.clone();

        assert_eq!(
            translate(VK_OEM_COMMA.0 as usize, 0, true, &hotkey),
            Some(InputEvent::PageNext)
        );
    }

    #[test]
    fn page_prev_works_for_non_default_key() {
        let mut hotkey = default_hotkey();
        hotkey.page_next = "period".to_string();
        hotkey.page_prev = "comma".to_string();

        assert_eq!(
            translate(VK_OEM_COMMA.0 as usize, 0, true, &hotkey),
            Some(InputEvent::PagePrev)
        );
    }

    #[test]
    fn page_key_non_default_minus_equal() {
        let mut hotkey = default_hotkey();
        hotkey.page_next = "minus".to_string();
        hotkey.page_prev = "equal".to_string();

        assert_eq!(
            translate(VK_OEM_MINUS.0 as usize, 0, true, &hotkey),
            Some(InputEvent::PageNext)
        );
        assert_eq!(
            translate(VK_OEM_PLUS.0 as usize, 0, true, &hotkey),
            Some(InputEvent::PagePrev)
        );
    }

    #[test]
    fn classify_shifted_punctuation_as_symbol() {
        assert_eq!(classify_printable_char('!', true), InputEvent::Symbol('！'));
    }

    #[test]
    fn classify_shifted_uppercase_as_code_char() {
        assert_eq!(classify_printable_char('A', true), InputEvent::Char('A'));
    }

    #[test]
    fn ascii_punctuation_maps_to_chinese_symbol() {
        assert_eq!(
            classify_printable_char(',', false),
            InputEvent::Symbol('，')
        );
        assert_eq!(classify_printable_char('!', true), InputEvent::Symbol('！'));
    }

    #[test]
    fn key_name_to_vk_maps_all_options() {
        assert_eq!(key_name_to_vk("comma"), Some(VK_OEM_COMMA.0));
        assert_eq!(key_name_to_vk("period"), Some(VK_OEM_PERIOD.0));
        assert_eq!(key_name_to_vk("semicolon"), Some(VK_OEM_1.0));
        assert_eq!(key_name_to_vk("quote"), Some(VK_OEM_7.0));
        assert_eq!(key_name_to_vk("minus"), Some(VK_OEM_MINUS.0));
        assert_eq!(key_name_to_vk("equal"), Some(VK_OEM_PLUS.0));
        assert_eq!(key_name_to_vk("space"), Some(VK_SPACE.0));
        assert_eq!(key_name_to_vk("left"), Some(VK_LEFT.0));
        assert_eq!(key_name_to_vk("right"), Some(VK_RIGHT.0));
        assert_eq!(key_name_to_vk("page_up"), Some(VK_PRIOR.0));
        assert_eq!(key_name_to_vk("page_down"), Some(VK_NEXT.0));
        assert_eq!(key_name_to_vk("unknown"), None);
    }

    #[test]
    fn selecting_state_maps_quick_select_hotkeys() {
        let hotkey = default_hotkey();

        assert_eq!(
            translate(VK_OEM_1.0 as usize, 0, true, &hotkey),
            Some(InputEvent::Select(2))
        );
        assert_eq!(
            translate(VK_OEM_7.0 as usize, 0, true, &hotkey),
            Some(InputEvent::Select(3))
        );
    }
}
