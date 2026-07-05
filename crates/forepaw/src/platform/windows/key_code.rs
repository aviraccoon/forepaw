//! Maps key names to Windows virtual-key (VK) codes.
//!
//! VK codes are the physical-key identifiers used by `SendInput` /
//! `KEYBDINPUT.wVk`. Raw hex values are embedded directly (with the Win32
//! symbol name in comments) to avoid pulling individual `VK_*` constants,
//! which the `windows` crate types as `u32` while `wVk` is `u16`.
//!
//! Reference: <https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>

/// Returns the Windows virtual-key code for a named key, or `None` if
/// unrecognized.
///
/// Key names are case-insensitive (the `KeyCombo` parser already lowercases,
/// but we tolerate raw input). Supports: return, tab, space, escape,
/// delete/backspace, forwarddelete, arrow keys, navigation keys, function keys
/// (f1-f12), single letters (a-z), and single digits (0-9).
///
/// Single letters map to `0x41..=0x5A` (`VK_A..VK_Z`) and single digits to
/// `0x30..=0x39` (`VK_0..VK_9`); named keys are matched explicitly.
#[must_use]
pub fn virtual_key_code(key: &str) -> Option<u16> {
    let lower = key.to_ascii_lowercase();

    // Fast path: a single ASCII alphanumeric maps directly to its VK code.
    // (Two-char tokens like "f1" fall through to the named-key match below.)
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_lowercase() {
            let vk = u16::from(c as u8) - u16::from(b'a') + 0x41;
            return Some(vk);
        }
        if c.is_ascii_digit() {
            let vk = u16::from(c as u8) - u16::from(b'0') + 0x30;
            return Some(vk);
        }
    }

    match lower.as_str() {
        // Special keys
        "return" | "enter" => Some(0x0D),     // VK_RETURN
        "tab" => Some(0x09),                  // VK_TAB
        "space" => Some(0x20),                // VK_SPACE
        "escape" | "esc" => Some(0x1B),       // VK_ESCAPE
        "delete" | "backspace" => Some(0x08), // VK_BACK
        "forwarddelete" => Some(0x2E),        // VK_DELETE

        // Arrow keys
        "up" => Some(0x26),    // VK_UP
        "down" => Some(0x28),  // VK_DOWN
        "left" => Some(0x25),  // VK_LEFT
        "right" => Some(0x27), // VK_RIGHT

        // Navigation
        "home" => Some(0x24),     // VK_HOME
        "end" => Some(0x23),      // VK_END
        "pageup" => Some(0x21),   // VK_PRIOR
        "pagedown" => Some(0x22), // VK_NEXT

        // Function keys
        "f1" => Some(0x70),
        "f2" => Some(0x71),
        "f3" => Some(0x72),
        "f4" => Some(0x73),
        "f5" => Some(0x74),
        "f6" => Some(0x75),
        "f7" => Some(0x76),
        "f8" => Some(0x77),
        "f9" => Some(0x78),
        "f10" => Some(0x79),
        "f11" => Some(0x7A),
        "f12" => Some(0x7B),

        _ => None,
    }
}

/// Returns the VK code for a modifier, or `None`.
///
/// `Command` maps to `VK_CONTROL` (0x11) — not the Windows key — so that
/// cross-platform shortcuts like `cmd+s` resolve to the platform's primary
/// modifier on Windows (Ctrl+S), matching macOS Cmd+S semantically. `Control`
/// is also `VK_CONTROL`. `Option`/Alt is `VK_MENU`. Shift is `VK_SHIFT`.
#[must_use]
pub fn modifier_vk(modifier: &crate::core::key_combo::Modifier) -> Option<u16> {
    use crate::core::key_combo::Modifier;
    match modifier {
        // Command -> Ctrl for cross-platform shortcut parity (see fn docs).
        Modifier::Command | Modifier::Control => Some(0x11), // VK_CONTROL
        Modifier::Option => Some(0x12),                      // VK_MENU (Alt)
        Modifier::Shift => Some(0x10),                       // VK_SHIFT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_keys() {
        assert_eq!(virtual_key_code("return"), Some(0x0D));
        assert_eq!(virtual_key_code("enter"), Some(0x0D));
        assert_eq!(virtual_key_code("tab"), Some(0x09));
        assert_eq!(virtual_key_code("space"), Some(0x20));
        assert_eq!(virtual_key_code("escape"), Some(0x1B));
        assert_eq!(virtual_key_code("esc"), Some(0x1B));
        assert_eq!(virtual_key_code("delete"), Some(0x08));
        assert_eq!(virtual_key_code("backspace"), Some(0x08));
        assert_eq!(virtual_key_code("forwarddelete"), Some(0x2E));
    }

    #[test]
    fn arrow_keys() {
        assert_eq!(virtual_key_code("up"), Some(0x26));
        assert_eq!(virtual_key_code("down"), Some(0x28));
        assert_eq!(virtual_key_code("left"), Some(0x25));
        assert_eq!(virtual_key_code("right"), Some(0x27));
    }

    #[test]
    fn navigation_keys() {
        assert_eq!(virtual_key_code("home"), Some(0x24));
        assert_eq!(virtual_key_code("end"), Some(0x23));
        assert_eq!(virtual_key_code("pageup"), Some(0x21));
        assert_eq!(virtual_key_code("pagedown"), Some(0x22));
    }

    #[test]
    fn function_keys() {
        assert_eq!(virtual_key_code("f1"), Some(0x70));
        assert_eq!(virtual_key_code("f12"), Some(0x7B));
    }

    #[test]
    fn letters_and_digits() {
        assert_eq!(virtual_key_code("a"), Some(0x41));
        assert_eq!(virtual_key_code("z"), Some(0x5A));
        assert_eq!(virtual_key_code("0"), Some(0x30));
        assert_eq!(virtual_key_code("9"), Some(0x39));
    }

    #[test]
    fn two_char_tokens_hit_named_match() {
        // "f1" must not be misread as the letter 'f'.
        assert_eq!(virtual_key_code("f1"), Some(0x70));
    }

    #[test]
    fn unknown_key() {
        assert_eq!(virtual_key_code("foobar"), None);
        assert_eq!(virtual_key_code("/"), None);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(virtual_key_code("Return"), Some(0x0D));
        assert_eq!(virtual_key_code("ESC"), Some(0x1B));
        assert_eq!(virtual_key_code("UP"), Some(0x26));
        assert_eq!(virtual_key_code("S"), Some(0x53));
    }

    #[test]
    fn modifier_mapping() {
        use crate::core::key_combo::Modifier;
        assert_eq!(modifier_vk(&Modifier::Command), Some(0x11));
        assert_eq!(modifier_vk(&Modifier::Control), Some(0x11));
        assert_eq!(modifier_vk(&Modifier::Option), Some(0x12));
        assert_eq!(modifier_vk(&Modifier::Shift), Some(0x10));
    }
}
