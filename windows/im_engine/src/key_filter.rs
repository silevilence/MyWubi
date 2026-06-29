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

/// 把 (`wparam`, `lparam`) 解析为通用 [`InputEvent`]；返回 `None` 表示该按键与本输入法无关。
///
/// `wparam` 为虚拟键码；`lparam` 仅用于判断过渡状态与扩展键（本实现中不直接使用，
/// 保留参数以契合回调签名）。
pub fn translate(wparam: usize, _lparam: isize) -> Option<InputEvent> {
    let vk = wparam as u16;
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

/// 仅作快速过滤用：当前按键是否为输入法可能关注的键。
pub fn is_intercepted_key(wparam: usize) -> bool {
    let vk = wparam as u16;
    (VK_A.0..=VK_Z.0).contains(&vk)
    || (VK_0.0..=VK_9.0).contains(&vk)
        || vk == VK_SPACE.0
        || vk == VK_RETURN.0
        || vk == VK_BACK.0
        || vk == VK_ESCAPE.0
    || vk == VK_OEM_1.0
    || vk == VK_OEM_PLUS.0
        || vk == VK_OEM_COMMA.0
    || vk == VK_OEM_MINUS.0
        || vk == VK_OEM_PERIOD.0
    || vk == VK_OEM_2.0
    || vk == VK_OEM_3.0
    || vk == VK_OEM_4.0
    || vk == VK_OEM_5.0
    || vk == VK_OEM_6.0
    || vk == VK_OEM_7.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_letter_lowercase() {
        assert_eq!(translate(VK_A.0 as usize, 0), Some(InputEvent::Char('a')));
        assert_eq!(translate(VK_Z.0 as usize, 0), Some(InputEvent::Char('z')));
    }

    #[test]
    fn translate_digit_chars() {
        assert_eq!(translate(VK_0.0 as usize, 0), Some(InputEvent::Char('0')));
        assert_eq!(translate(VK_1.0 as usize, 0), Some(InputEvent::Char('1')));
        assert_eq!(translate(VK_9.0 as usize, 0), Some(InputEvent::Char('9')));
    }

    #[test]
    fn translate_space_and_enter() {
        assert_eq!(translate(VK_SPACE.0 as usize, 0), Some(InputEvent::Space));
        assert_eq!(translate(VK_RETURN.0 as usize, 0), Some(InputEvent::Enter));
    }

    #[test]
    fn translate_backspace_and_esc() {
        assert_eq!(translate(VK_BACK.0 as usize, 0), Some(InputEvent::Backspace));
        assert_eq!(translate(VK_ESCAPE.0 as usize, 0), Some(InputEvent::Esc));
    }

    #[test]
    fn translate_common_punctuation_keys() {
        assert_eq!(translate(VK_OEM_COMMA.0 as usize, 0), Some(InputEvent::Char(',')));
        assert_eq!(translate(VK_OEM_PERIOD.0 as usize, 0), Some(InputEvent::Char('.')));
        assert_eq!(translate(VK_OEM_5.0 as usize, 0), Some(InputEvent::Char('\\')));
        assert_eq!(translate(VK_OEM_2.0 as usize, 0), Some(InputEvent::Char('/')));
    }

    #[test]
    fn translate_unknown_returns_none() {
        assert_eq!(translate(0xFF as usize, 0), None);
    }
}