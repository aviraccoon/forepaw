//! Maps key names to macOS virtual key codes.
//!
//! These are the same values defined in Carbon's `HIToolbox/Events.h`.
//! We embed them directly to avoid depending on the Carbon framework.

/// Returns the macOS virtual key code for a named key, or `None` if unrecognized.
///
/// Key names are case-insensitive. Supports: return, tab, space, escape,
/// delete, forwarddelete, arrow keys (up/down/left/right), function keys
/// (f1-f12), navigation keys (home, end, pageup, pagedown), letters (a-z),
/// and digits (0-9).
pub fn virtual_key_code(key: &str) -> Option<u16> {
    // Match on lowercase. The key names are short enough that allocation-free
    // ASCII lowering isn't worth the complexity -- just use to_ascii_lowercase.
    match key.to_ascii_lowercase().as_str() {
        // Special keys
        "return" | "enter" => Some(36),
        "tab" => Some(48),
        "space" => Some(49),
        "escape" | "esc" => Some(53),
        "delete" | "backspace" => Some(51),
        "forwarddelete" => Some(117),

        // Arrow keys
        "up" => Some(126),
        "down" => Some(125),
        "left" => Some(123),
        "right" => Some(124),

        // Navigation
        "home" => Some(115),
        "end" => Some(119),
        "pageup" => Some(116),
        "pagedown" => Some(121),

        // Function keys
        "f1" => Some(122),
        "f2" => Some(120),
        "f3" => Some(99),
        "f4" => Some(118),
        "f5" => Some(96),
        "f6" => Some(97),
        "f7" => Some(98),
        "f8" => Some(100),
        "f9" => Some(101),
        "f10" => Some(109),
        "f11" => Some(103),
        "f12" => Some(111),

        // Letters
        "a" => Some(0),
        "b" => Some(11),
        "c" => Some(8),
        "d" => Some(2),
        "e" => Some(14),
        "f" => Some(3),
        "g" => Some(5),
        "h" => Some(4),
        "i" => Some(34),
        "j" => Some(38),
        "k" => Some(40),
        "l" => Some(37),
        "m" => Some(46),
        "n" => Some(45),
        "o" => Some(31),
        "p" => Some(35),
        "q" => Some(12),
        "r" => Some(15),
        "s" => Some(1),
        "t" => Some(17),
        "u" => Some(32),
        "v" => Some(9),
        "w" => Some(13),
        "x" => Some(7),
        "y" => Some(16),
        "z" => Some(6),

        // Digits
        "0" => Some(29),
        "1" => Some(18),
        "2" => Some(19),
        "3" => Some(20),
        "4" => Some(21),
        "5" => Some(23),
        "6" => Some(22),
        "7" => Some(26),
        "8" => Some(28),
        "9" => Some(25),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_special_keys() {
        assert_eq!(virtual_key_code("return"), Some(36));
        assert_eq!(virtual_key_code("enter"), Some(36));
        assert_eq!(virtual_key_code("tab"), Some(48));
        assert_eq!(virtual_key_code("space"), Some(49));
        assert_eq!(virtual_key_code("escape"), Some(53));
        assert_eq!(virtual_key_code("esc"), Some(53));
        assert_eq!(virtual_key_code("delete"), Some(51));
        assert_eq!(virtual_key_code("backspace"), Some(51));
        assert_eq!(virtual_key_code("forwarddelete"), Some(117));
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(virtual_key_code("up"), Some(126));
        assert_eq!(virtual_key_code("down"), Some(125));
        assert_eq!(virtual_key_code("left"), Some(123));
        assert_eq!(virtual_key_code("right"), Some(124));
    }

    #[test]
    fn test_navigation_keys() {
        assert_eq!(virtual_key_code("home"), Some(115));
        assert_eq!(virtual_key_code("end"), Some(119));
        assert_eq!(virtual_key_code("pageup"), Some(116));
        assert_eq!(virtual_key_code("pagedown"), Some(121));
    }

    #[test]
    fn test_function_keys() {
        assert_eq!(virtual_key_code("f1"), Some(122));
        assert_eq!(virtual_key_code("f12"), Some(111));
    }

    #[test]
    fn test_letters() {
        assert_eq!(virtual_key_code("a"), Some(0));
        assert_eq!(virtual_key_code("z"), Some(6));
    }

    #[test]
    fn test_digits() {
        assert_eq!(virtual_key_code("0"), Some(29));
        assert_eq!(virtual_key_code("9"), Some(25));
    }

    #[test]
    fn test_unknown_key() {
        assert_eq!(virtual_key_code("foobar"), None);
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(virtual_key_code("Return"), Some(36));
        assert_eq!(virtual_key_code("ESC"), Some(53));
        assert_eq!(virtual_key_code("UP"), Some(126));
    }
}
