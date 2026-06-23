//! 按键事件映射：将 Windows 虚拟键码 / LPARAM 转换为 [`core_engine::InputEvent`]。
//!
//! [ITfKeyEventSink::OnKeyDown] 回调中传入 `wparam` 即虚拟键码 (VK_*),
//! `lparam` 包含重复次数、扫描码及扩展键标志位。本模块负责把常见的输入法相关
//! 按键翻译为跨平台 [`InputEvent`]，其余返回 `None`（视为 passthrough）。
//!
//! 翻译规则对应 ROADMAP“按键拦截规则”：
//! * 字母键 (A-Z) → 缓冲为编码字符（小写化）；
//! * 数字键 1..=9 → 候选词选择；
//! * 空格键 → 空格首选；
//! * 回车键 → 回车上屏原始编码；
//! * 退格键 → 删除最后一个编码字符；
//! * Esc 键 → 清空缓冲；
//! * 逗号 / 句号 → 翻页。

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

        // 数字 1..=9 选择第 N 个候选词
        v if (VK_1.0..=VK_9.0).contains(&v) => {
            Some(InputEvent::Select((v - VK_0.0) as usize))
        }

        // 空格首选上屏
        v if v == VK_SPACE.0 => Some(InputEvent::Space),

        // 回车上屏原始编码
        v if v == VK_RETURN.0 => Some(InputEvent::Enter),

        // 退格：删除最后编码
        v if v == VK_BACK.0 => Some(InputEvent::Backspace),

        // Esc 清空
        v if v == VK_ESCAPE.0 => Some(InputEvent::Esc),

        // 默认翻页键：逗号上一页 / 句号下一页
        v if v == VK_OEM_COMMA.0 => Some(InputEvent::PagePrev),
        v if v == VK_OEM_PERIOD.0 => Some(InputEvent::PageNext),

        _ => None,
    }
}

/// 仅作快速过滤用：当前按键是否为输入法可能关注的键。
pub fn is_intercepted_key(wparam: usize) -> bool {
    let vk = wparam as u16;
    (VK_A.0..=VK_Z.0).contains(&vk)
        || (VK_1.0..=VK_9.0).contains(&vk)
        || vk == VK_SPACE.0
        || vk == VK_RETURN.0
        || vk == VK_BACK.0
        || vk == VK_ESCAPE.0
        || vk == VK_OEM_COMMA.0
        || vk == VK_OEM_PERIOD.0
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
    fn translate_digit_select() {
        assert_eq!(translate(VK_1.0 as usize, 0), Some(InputEvent::Select(1)));
        assert_eq!(translate(VK_9.0 as usize, 0), Some(InputEvent::Select(9)));
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
    fn translate_page_keys() {
        assert_eq!(
            translate(VK_OEM_COMMA.0 as usize, 0),
            Some(InputEvent::PagePrev)
        );
        assert_eq!(
            translate(VK_OEM_PERIOD.0 as usize, 0),
            Some(InputEvent::PageNext)
        );
    }

    #[test]
    fn translate_unknown_returns_none() {
        assert_eq!(translate(0xFF as usize, 0), None);
    }
}