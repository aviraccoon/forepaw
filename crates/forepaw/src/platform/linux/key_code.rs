//! Maps key names and characters to evdev key codes (`/dev/uinput`).
//!
//! evdev codes are the physical-key identifiers used by the Linux input
//! subsystem. They follow the AT keyboard layout (NOT alphabetical, NOT the
//! USB HID or Win32 VK codes), so letters/digits are matched explicitly
//! rather than computed from an offset.
//!
//! Source: `include/uapi/linux/input-event-codes.h` (verified against
//! linux-headers-6.18.7). Values are frozen kernel ABI.

use crate::core::key_combo::Modifier;

/// A single key press: the evdev code plus whether Shift must be held.
///
/// `keyboard_type` decomposes a string into a sequence of these so it can be
/// emitted over uinput. `shift` is true for uppercase letters and the
/// shifted symbol on each US-layout key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    /// evdev `KEY_*` code.
    pub code: u16,
    /// Whether Shift must be held to produce the target character.
    pub shift: bool,
}

/// Returns the evdev key code for a named key, or `None` if unrecognized.
///
/// Key names are case-insensitive. Supports: return, tab, space, escape,
/// delete/backspace, forwarddelete, arrow keys, navigation keys, function
/// keys (f1-f12), single letters (a-z), and single digits (0-9).
///
/// Matches the macOS/Windows key-code tables so `KeyCombo` names resolve to
/// the same logical key on every platform.
#[must_use]
pub fn evdev_key_code(key: &str) -> Option<u16> {
    match key.to_ascii_lowercase().as_str() {
        // Special keys
        "return" | "enter" => Some(28),     // KEY_ENTER
        "tab" => Some(15),                  // KEY_TAB
        "space" => Some(57),                // KEY_SPACE
        "escape" | "esc" => Some(1),        // KEY_ESC
        "delete" | "backspace" => Some(14), // KEY_BACKSPACE
        "forwarddelete" => Some(111),       // KEY_DELETE

        // Arrow keys
        "up" => Some(103),    // KEY_UP
        "down" => Some(108),  // KEY_DOWN
        "left" => Some(105),  // KEY_LEFT
        "right" => Some(106), // KEY_RIGHT

        // Navigation
        "home" => Some(102),     // KEY_HOME
        "end" => Some(107),      // KEY_END
        "pageup" => Some(104),   // KEY_PAGEUP
        "pagedown" => Some(109), // KEY_PAGEDOWN

        // Function keys
        "f1" => Some(59),
        "f2" => Some(60),
        "f3" => Some(61),
        "f4" => Some(62),
        "f5" => Some(63),
        "f6" => Some(64),
        "f7" => Some(65),
        "f8" => Some(66),
        "f9" => Some(67),
        "f10" => Some(68),
        "f11" => Some(87),
        "f12" => Some(88),

        // Letters (AT-keyboard layout: not sequential)
        "a" => Some(30),
        "b" => Some(48),
        "c" => Some(46),
        "d" => Some(32),
        "e" => Some(18),
        "f" => Some(33),
        "g" => Some(34),
        "h" => Some(35),
        "i" => Some(23),
        "j" => Some(36),
        "k" => Some(37),
        "l" => Some(38),
        "m" => Some(50),
        "n" => Some(49),
        "o" => Some(24),
        "p" => Some(25),
        "q" => Some(16),
        "r" => Some(19),
        "s" => Some(31),
        "t" => Some(20),
        "u" => Some(22),
        "v" => Some(47),
        "w" => Some(17),
        "x" => Some(45),
        "y" => Some(21),
        "z" => Some(44),

        // Digits (top row)
        "0" => Some(11),
        "1" => Some(2),
        "2" => Some(3),
        "3" => Some(4),
        "4" => Some(5),
        "5" => Some(6),
        "6" => Some(7),
        "7" => Some(8),
        "8" => Some(9),
        "9" => Some(10),

        _ => None,
    }
}

/// Returns the evdev code for a modifier, or `None`.
///
/// `Command` maps to `KEY_LEFTCTRL` — not the Super/Meta key — so that
/// cross-platform shortcuts like `cmd+s` resolve to the platform's primary
/// modifier on Linux (Ctrl+S), matching macOS Cmd+S and Windows Ctrl+S
/// semantically. `Control` is also `KEY_LEFTCTRL`. `Option`/Alt is
/// `KEY_LEFTALT`. Shift is `KEY_LEFTSHIFT`.
#[must_use]
pub fn modifier_code(modifier: &Modifier) -> Option<u16> {
    match modifier {
        // Command -> Ctrl for cross-platform shortcut parity (see fn docs).
        Modifier::Command | Modifier::Control => Some(29), // KEY_LEFTCTRL
        Modifier::Option => Some(56),                      // KEY_LEFTALT
        Modifier::Shift => Some(42),                       // KEY_LEFTSHIFT
    }
}

/// Map a single character to the evdev key press that produces it on a US
/// QWERTY layout, or `None` if it can't be typed via raw keycodes.
///
/// Covers printable ASCII plus newline (`\n` -> Enter) and tab (`\t` -> Tab).
/// Returns `None` for non-ASCII (the active XKB layout determines what a
/// keycode produces, so raw keycodes cannot express arbitrary Unicode);
/// `keyboard_type` then falls back to skipping the character. Non-ASCII text
/// needs a clipboard-paste path (deferred).
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "US-layout char-to-keycode match is a large lookup table"
)]
pub fn char_to_evdev(c: char) -> Option<KeyStroke> {
    // Control characters that map to named keys.
    let (code, shift) = match c {
        '\n' => (28, false), // KEY_ENTER
        '\t' => (15, false), // KEY_TAB
        // Printable ASCII on the US layout.
        ' ' => (57, false),  // KEY_SPACE
        '!' => (2, true),    // KEY_1
        '"' => (40, true),   // KEY_APOSTROPHE
        '#' => (4, true),    // KEY_3
        '$' => (5, true),    // KEY_4
        '%' => (6, true),    // KEY_5
        '&' => (8, true),    // KEY_7
        '\'' => (40, false), // KEY_APOSTROPHE
        '(' => (10, true),   // KEY_9
        ')' => (11, true),   // KEY_0
        '*' => (9, true),    // KEY_8
        '+' => (13, true),   // KEY_EQUAL
        ',' => (51, false),  // KEY_COMMA
        '-' => (12, false),  // KEY_MINUS
        '.' => (52, false),  // KEY_DOT
        '/' => (53, false),  // KEY_SLASH
        '0' => (11, false),
        '1' => (2, false),
        '2' => (3, false),
        '3' => (4, false),
        '4' => (5, false),
        '5' => (6, false),
        '6' => (7, false),
        '7' => (8, false),
        '8' => (9, false),
        '9' => (10, false),
        ':' => (39, true),  // KEY_SEMICOLON
        ';' => (39, false), // KEY_SEMICOLON
        '<' => (51, true),  // KEY_COMMA
        '=' => (13, false), // KEY_EQUAL
        '>' => (52, true),  // KEY_DOT
        '?' => (53, true),  // KEY_SLASH
        '@' => (3, true),   // KEY_2
        'A' => (30, true),
        'B' => (48, true),
        'C' => (46, true),
        'D' => (32, true),
        'E' => (18, true),
        'F' => (33, true),
        'G' => (34, true),
        'H' => (35, true),
        'I' => (23, true),
        'J' => (36, true),
        'K' => (37, true),
        'L' => (38, true),
        'M' => (50, true),
        'N' => (49, true),
        'O' => (24, true),
        'P' => (25, true),
        'Q' => (16, true),
        'R' => (19, true),
        'S' => (31, true),
        'T' => (20, true),
        'U' => (22, true),
        'V' => (47, true),
        'W' => (17, true),
        'X' => (45, true),
        'Y' => (21, true),
        'Z' => (44, true),
        '[' => (26, false),  // KEY_LEFTBRACE
        '\\' => (43, false), // KEY_BACKSLASH
        ']' => (27, false),  // KEY_RIGHTBRACE
        '^' => (7, true),    // KEY_6
        '_' => (12, true),   // KEY_MINUS
        '`' => (41, false),  // KEY_GRAVE
        'a' => (30, false),
        'b' => (48, false),
        'c' => (46, false),
        'd' => (32, false),
        'e' => (18, false),
        'f' => (33, false),
        'g' => (34, false),
        'h' => (35, false),
        'i' => (23, false),
        'j' => (36, false),
        'k' => (37, false),
        'l' => (38, false),
        'm' => (50, false),
        'n' => (49, false),
        'o' => (24, false),
        'p' => (25, false),
        'q' => (16, false),
        'r' => (19, false),
        's' => (31, false),
        't' => (20, false),
        'u' => (22, false),
        'v' => (47, false),
        'w' => (17, false),
        'x' => (45, false),
        'y' => (21, false),
        'z' => (44, false),
        '{' => (26, true), // KEY_LEFTBRACE
        '|' => (43, true), // KEY_BACKSLASH
        '}' => (27, true), // KEY_RIGHTBRACE
        '~' => (41, true), // KEY_GRAVE
        // \r and other control chars: drop silently (None).
        _ => return None,
    };
    Some(KeyStroke { code, shift })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_keys() {
        assert_eq!(evdev_key_code("return"), Some(28));
        assert_eq!(evdev_key_code("enter"), Some(28));
        assert_eq!(evdev_key_code("tab"), Some(15));
        assert_eq!(evdev_key_code("space"), Some(57));
        assert_eq!(evdev_key_code("escape"), Some(1));
        assert_eq!(evdev_key_code("esc"), Some(1));
        assert_eq!(evdev_key_code("delete"), Some(14));
        assert_eq!(evdev_key_code("backspace"), Some(14));
        assert_eq!(evdev_key_code("forwarddelete"), Some(111));
    }

    #[test]
    fn arrow_keys() {
        assert_eq!(evdev_key_code("up"), Some(103));
        assert_eq!(evdev_key_code("down"), Some(108));
        assert_eq!(evdev_key_code("left"), Some(105));
        assert_eq!(evdev_key_code("right"), Some(106));
    }

    #[test]
    fn navigation_keys() {
        assert_eq!(evdev_key_code("home"), Some(102));
        assert_eq!(evdev_key_code("end"), Some(107));
        assert_eq!(evdev_key_code("pageup"), Some(104));
        assert_eq!(evdev_key_code("pagedown"), Some(109));
    }

    #[test]
    fn function_keys() {
        assert_eq!(evdev_key_code("f1"), Some(59));
        assert_eq!(evdev_key_code("f12"), Some(88));
    }

    #[test]
    fn letters() {
        assert_eq!(evdev_key_code("a"), Some(30));
        assert_eq!(evdev_key_code("z"), Some(44));
        assert_eq!(evdev_key_code("m"), Some(50));
    }

    #[test]
    fn digits() {
        assert_eq!(evdev_key_code("0"), Some(11));
        assert_eq!(evdev_key_code("1"), Some(2));
        assert_eq!(evdev_key_code("9"), Some(10));
    }

    #[test]
    fn unknown_key() {
        assert_eq!(evdev_key_code("foobar"), None);
        assert_eq!(evdev_key_code("/"), None);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(evdev_key_code("Return"), Some(28));
        assert_eq!(evdev_key_code("ESC"), Some(1));
        assert_eq!(evdev_key_code("S"), Some(31));
    }

    #[test]
    fn modifier_mapping() {
        assert_eq!(modifier_code(&Modifier::Command), Some(29));
        assert_eq!(modifier_code(&Modifier::Control), Some(29));
        assert_eq!(modifier_code(&Modifier::Option), Some(56));
        assert_eq!(modifier_code(&Modifier::Shift), Some(42));
    }

    #[test]
    fn char_lowercase_letter() {
        assert_eq!(
            char_to_evdev('a'),
            Some(KeyStroke {
                code: 30,
                shift: false
            })
        );
    }

    #[test]
    fn char_uppercase_letter_shifts() {
        assert_eq!(
            char_to_evdev('Z'),
            Some(KeyStroke {
                code: 44,
                shift: true
            })
        );
    }

    #[test]
    fn char_digit_and_symbol_share_key() {
        // '5' and '%' are the same key; '%' adds Shift.
        assert_eq!(
            char_to_evdev('5'),
            Some(KeyStroke {
                code: 6,
                shift: false
            })
        );
        assert_eq!(
            char_to_evdev('%'),
            Some(KeyStroke {
                code: 6,
                shift: true
            })
        );
    }

    #[test]
    fn char_control_chars() {
        assert_eq!(
            char_to_evdev('\n'),
            Some(KeyStroke {
                code: 28,
                shift: false
            })
        );
        assert_eq!(
            char_to_evdev('\t'),
            Some(KeyStroke {
                code: 15,
                shift: false
            })
        );
        // \r is silently dropped.
        assert_eq!(char_to_evdev('\r'), None);
    }

    #[test]
    fn char_non_ascii_unsupported() {
        assert_eq!(char_to_evdev('é'), None);
        assert_eq!(char_to_evdev('😀'), None);
    }

    #[test]
    fn char_full_printable_ascii_round_trips_to_a_stroke() {
        // Every printable ASCII char (0x20..0x7e) maps to a stroke; this is
        // a guard against silently dropping a mappable character.
        for code in 0x20_u32..=0x7e {
            let c = char::from_u32(code).expect("valid ASCII");
            assert!(
                char_to_evdev(c).is_some(),
                "printable ASCII {c:?} (0x{code:02x}) has no evdev mapping"
            );
        }
    }
}
