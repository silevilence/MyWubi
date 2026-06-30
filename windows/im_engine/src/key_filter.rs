//! 按键事件映射：将 Windows 虚拟键码 / LPARAM 转换为 [`core_engine::InputEvent`]。
//!
//! [ITfKeyEventSink::OnKeyDown] 回调中传入 `wparam` 即虚拟键码 (VK_*),
//! `lparam` 包含重复次数、扫描码及扩展键标志位。本模块负责把常见的输入法相关
//! 按键翻译为跨平台 [`InputEvent`]，其余返回 `None`（视为 passthrough）。
//!
//! 翻译规则对应 ROADMAP“按键拦截规则”：
//! * 字母键 (A-Z) → 缓冲为编码字符（小写化）；
//! * 可打印标点键 → 原样回传字符，具体是缓冲还是直接上屏由状态机决定；
//! * 数字键 → 原样回传字符，状态机可在选词态把 1..=9 解释为候选选择；
//! * 空格键 → 空格首选；
//! * 回车键 → 回车上屏原始编码；
//! * 退格键 → 删除最后一个编码字符；
//! * Esc 键 → 清空缓冲；

use core_engine::InputEvent;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

/// 配置中翻页键的字符串标识 → 虚拟键码映射。
fn key_name_to_vk(name: &str) -> Option<u16> {
    match name {
        "comma" => Some(VK_OEM_COMMA.0),
        "period" => Some(VK_OEM_PERIOD.0),
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

/// 把 (`wparam`, `lparam`) 解析为通用 [`InputEvent`]；返回 `None` 表示该按键与本输入法无关。
///
/// `wparam` 为虚拟键码；`lparam` 仅用于判断过渡状态与扩展键（本实现中不直接使用，
/// 保留参数以契合回调签名）。
pub fn translate(wparam: usize, _lparam: isize, page_next: &str, page_prev: &str) -> Option<InputEvent> {
    let vk = wparam as u16;
    // 翻页键优先匹配：若当前按键被配置为翻页键，则返回翻页事件。
    if let Some(pn_vk) = key_name_to_vk(page_next) {
        if vk == pn_vk {
            return Some(InputEvent::PageNext);
        }
    }
    if let Some(pp_vk) = key_name_to_vk(page_prev) {
        if vk == pp_vk {
            return Some(InputEvent::PagePrev);
        }
    }
    match vk {
        // 字母键：A..Z → 小写 c
        v if (VK_A.0..=VK_Z.0).contains(&v) => {
            Some(InputEvent::Char((b'a' + (v - VK_A.0) as u8) as char))
        }

        // 数字键：保留原始字符，由状态机判定是否作为候选快捷键。
        v if (VK_0.0..=VK_9.0).contains(&v) => {
            Some(InputEvent::Char((b'0' + (v - VK_0.0) as u8) as char))
        }

        // 空格首选上屏
        v if v == VK_SPACE.0 => Some(InputEvent::Space),

        // 回车上屏原始编码
        v if v == VK_RETURN.0 => Some(InputEvent::Enter),

        // 退格：删除最后编码
        v if v == VK_BACK.0 => Some(InputEvent::Backspace),

        // Esc 清空
        v if v == VK_ESCAPE.0 => Some(InputEvent::Esc),

        // 常见 ASCII 标点按键：原样回传字符。
        v if v == VK_OEM_1.0 => Some(InputEvent::Char(';')),
        v if v == VK_OEM_PLUS.0 => Some(InputEvent::Char('=')),
        v if v == VK_OEM_COMMA.0 => Some(InputEvent::Char(',')),
        v if v == VK_OEM_MINUS.0 => Some(InputEvent::Char('-')),
        v if v == VK_OEM_PERIOD.0 => Some(InputEvent::Char('.')),
        v if v == VK_OEM_2.0 => Some(InputEvent::Char('/')),
        v if v == VK_OEM_3.0 => Some(InputEvent::Char('`')),
        v if v == VK_OEM_4.0 => Some(InputEvent::Char('[')),
        v if v == VK_OEM_5.0 => Some(InputEvent::Char('\\')),
        v if v == VK_OEM_6.0 => Some(InputEvent::Char(']')),
        v if v == VK_OEM_7.0 => Some(InputEvent::Char('\'')),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PLUS};

    #[test]
    fn translate_letter_lowercase() {
        assert_eq!(translate(VK_A.0 as usize, 0, "comma", "period"), Some(InputEvent::Char('a')));
        assert_eq!(translate(VK_Z.0 as usize, 0, "comma", "period"), Some(InputEvent::Char('z')));
    }

    #[test]
    fn translate_digit_chars() {
        assert_eq!(translate(VK_0.0 as usize, 0, "comma", "period"), Some(InputEvent::Char('0')));
        assert_eq!(translate(VK_1.0 as usize, 0, "comma", "period"), Some(InputEvent::Char('1')));
        assert_eq!(translate(VK_9.0 as usize, 0, "comma", "period"), Some(InputEvent::Char('9')));
    }

    #[test]
    fn translate_space_and_enter() {
        assert_eq!(translate(VK_SPACE.0 as usize, 0, "comma", "period"), Some(InputEvent::Space));
        assert_eq!(translate(VK_RETURN.0 as usize, 0, "comma", "period"), Some(InputEvent::Enter));
    }

    #[test]
    fn translate_backspace_and_esc() {
        assert_eq!(translate(VK_BACK.0 as usize, 0, "comma", "period"), Some(InputEvent::Backspace));
        assert_eq!(translate(VK_ESCAPE.0 as usize, 0, "comma", "period"), Some(InputEvent::Esc));
    }

    #[test]
    fn translate_common_punctuation_keys() {
        // 默认配置下逗号/句号被映射为翻页键，不再返回标点字符
        assert_eq!(translate(VK_OEM_COMMA.0 as usize, 0, "comma", "period"), Some(InputEvent::PageNext));
        assert_eq!(translate(VK_OEM_PERIOD.0 as usize, 0, "comma", "period"), Some(InputEvent::PagePrev));
        // 未配置为翻页的标点键仍返回字符
        assert_eq!(translate(VK_OEM_5.0 as usize, 0, "comma", "period"), Some(InputEvent::Char('\\')));
        assert_eq!(translate(VK_OEM_2.0 as usize, 0, "comma", "period"), Some(InputEvent::Char('/')));
    }

    #[test]
    fn translate_unknown_returns_none() {
        assert_eq!(translate(0xFF as usize, 0, "comma", "period"), None);
    }

    #[test]
    fn page_next_takes_priority_over_page_prev() {
        // 同一个键配置为 page_next 和 page_prev 时，PageNext 先匹配
        assert_eq!(
            translate(VK_OEM_COMMA.0 as usize, 0, "comma", "comma"),
            Some(InputEvent::PageNext)
        );
    }

    #[test]
    fn page_prev_works_for_non_default_key() {
        // page_prev 配置为逗号键时，按逗号应返回 PagePrev
        assert_eq!(
            translate(VK_OEM_COMMA.0 as usize, 0, "period", "comma"),
            Some(InputEvent::PagePrev)
        );
    }

    #[test]
    fn page_key_non_default_minus_equal() {
        assert_eq!(
            translate(VK_OEM_MINUS.0 as usize, 0, "minus", "equal"),
            Some(InputEvent::PageNext)
        );
        assert_eq!(
            translate(VK_OEM_PLUS.0 as usize, 0, "minus", "equal"),
            Some(InputEvent::PagePrev)
        );
        // 未配置为翻页的逗号/句号回归标点字符
        assert_eq!(
            translate(VK_OEM_COMMA.0 as usize, 0, "minus", "equal"),
            Some(InputEvent::Char(','))
        );
    }

    #[test]
    fn page_key_space_overrides_space_input() {
        // 空格配为翻页键时，按空格返回 PageNext 而非 Space
        assert_eq!(
            translate(VK_SPACE.0 as usize, 0, "space", "period"),
            Some(InputEvent::PageNext)
        );
    }

    #[test]
    fn key_name_to_vk_maps_all_options() {
        assert_eq!(key_name_to_vk("comma"), Some(VK_OEM_COMMA.0));
        assert_eq!(key_name_to_vk("period"), Some(VK_OEM_PERIOD.0));
        assert_eq!(key_name_to_vk("minus"), Some(VK_OEM_MINUS.0));
        assert_eq!(key_name_to_vk("equal"), Some(VK_OEM_PLUS.0));
        assert_eq!(key_name_to_vk("space"), Some(VK_SPACE.0));
        assert_eq!(key_name_to_vk("left"), Some(VK_LEFT.0));
        assert_eq!(key_name_to_vk("right"), Some(VK_RIGHT.0));
        assert_eq!(key_name_to_vk("page_up"), Some(VK_PRIOR.0));
        assert_eq!(key_name_to_vk("page_down"), Some(VK_NEXT.0));
        assert_eq!(key_name_to_vk("unknown"), None);
    }
}