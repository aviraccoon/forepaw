/// Key combo parsing: "cmd+shift+s" -> KeyCombo { key, modifiers }.
///
/// Keyboard modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Command,
    Shift,
    Option,
    Control,
}

impl Modifier {
    /// Parse a single modifier string (case-insensitive).
    pub fn parse_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cmd" | "command" | "meta" | "super" => Some(Self::Command),
            "shift" => Some(Self::Shift),
            "opt" | "option" | "alt" => Some(Self::Option),
            "ctrl" | "control" => Some(Self::Control),
            _ => None,
        }
    }

    /// Parse a "+"-separated modifier string like "shift+alt".
    /// Returns an empty array for empty input.
    pub fn parse_modifiers(s: Option<&str>) -> Vec<Self> {
        let s = match s {
            Some(s) if !s.is_empty() => s,
            _ => return Vec::new(),
        };
        s.to_lowercase()
            .split('+')
            .filter_map(|part| Self::parse_name(part.trim()))
            .collect()
    }
}

/// A key combination (key + optional modifiers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    pub key: String,
    pub modifiers: Vec<Modifier>,
}

impl KeyCombo {
    pub fn new(key: impl Into<String>, modifiers: Vec<Modifier>) -> Self {
        Self {
            key: key.into(),
            modifiers,
        }
    }

    /// Parse a combo string like "cmd+shift+s" or "return".
    pub fn parse(s: &str) -> Self {
        let lower = s.to_lowercase();
        let parts: Vec<&str> = lower.split('+').collect();
        let mut modifiers: Vec<Modifier> = Vec::new();
        let mut key = String::new();

        for part in parts {
            if let Some(modifier) = Modifier::parse_name(part) {
                modifiers.push(modifier);
            } else {
                key = part.to_string();
            }
        }

        // If no non-modifier key was found, the whole string is the key
        if key.is_empty() {
            key = s.to_lowercase();
            modifiers.clear();
        }

        Self { key, modifiers }
    }
}

/// Mouse button for click actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
}

/// Click behavior modifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickOptions {
    pub button: MouseButton,
    pub click_count: u32,
}

impl ClickOptions {
    pub fn new(button: MouseButton, click_count: u32) -> Self {
        Self {
            button,
            click_count,
        }
    }

    pub fn normal() -> Self {
        Self {
            button: MouseButton::Left,
            click_count: 1,
        }
    }

    pub fn right_click() -> Self {
        Self {
            button: MouseButton::Right,
            click_count: 1,
        }
    }

    pub fn double_click() -> Self {
        Self {
            button: MouseButton::Left,
            click_count: 2,
        }
    }
}

impl Default for ClickOptions {
    fn default() -> Self {
        Self::normal()
    }
}

/// Options for drag operations.
#[derive(Debug, Clone)]
pub struct DragOptions {
    pub steps: u32,
    pub duration: f64,
    pub modifiers: Vec<Modifier>,
    pub pressure: Option<f64>,
    pub right_button: bool,
    pub close_path: bool,
}

impl Default for DragOptions {
    fn default() -> Self {
        Self {
            steps: 30,
            duration: 0.3,
            modifiers: Vec::new(),
            pressure: None,
            right_button: false,
            close_path: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_key() {
        let combo = KeyCombo::parse("return");
        assert_eq!(combo.key, "return");
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn single_modifier() {
        let combo = KeyCombo::parse("cmd+s");
        assert_eq!(combo.key, "s");
        assert_eq!(combo.modifiers, vec![Modifier::Command]);
    }

    #[test]
    fn multiple_modifiers() {
        let combo = KeyCombo::parse("cmd+shift+s");
        assert_eq!(combo.key, "s");
        assert!(combo.modifiers.contains(&Modifier::Command));
        assert!(combo.modifiers.contains(&Modifier::Shift));
        assert_eq!(combo.modifiers.len(), 2);
    }

    #[test]
    fn modifier_aliases() {
        assert_eq!(KeyCombo::parse("cmd+a").modifiers, vec![Modifier::Command]);
        assert_eq!(
            KeyCombo::parse("command+a").modifiers,
            vec![Modifier::Command]
        );
        assert_eq!(KeyCombo::parse("meta+a").modifiers, vec![Modifier::Command]);
        assert_eq!(
            KeyCombo::parse("super+a").modifiers,
            vec![Modifier::Command]
        );

        assert_eq!(KeyCombo::parse("opt+a").modifiers, vec![Modifier::Option]);
        assert_eq!(
            KeyCombo::parse("option+a").modifiers,
            vec![Modifier::Option]
        );
        assert_eq!(KeyCombo::parse("alt+a").modifiers, vec![Modifier::Option]);

        assert_eq!(KeyCombo::parse("ctrl+a").modifiers, vec![Modifier::Control]);
        assert_eq!(
            KeyCombo::parse("control+a").modifiers,
            vec![Modifier::Control]
        );

        assert_eq!(KeyCombo::parse("shift+a").modifiers, vec![Modifier::Shift]);
    }

    #[test]
    fn case_insensitive() {
        let combo = KeyCombo::parse("CMD+Shift+S");
        assert_eq!(combo.key, "s");
        assert!(combo.modifiers.contains(&Modifier::Command));
        assert!(combo.modifiers.contains(&Modifier::Shift));
    }

    #[test]
    fn four_modifiers() {
        let combo = KeyCombo::parse("cmd+shift+opt+ctrl+z");
        assert_eq!(combo.key, "z");
        assert_eq!(combo.modifiers.len(), 4);
    }

    #[test]
    fn parse_modifiers_nil() {
        assert!(Modifier::parse_modifiers(None).is_empty());
    }

    #[test]
    fn parse_modifiers_empty() {
        assert!(Modifier::parse_modifiers(Some("")).is_empty());
    }

    #[test]
    fn parse_modifiers_single() {
        assert_eq!(
            Modifier::parse_modifiers(Some("shift")),
            vec![Modifier::Shift]
        );
    }

    #[test]
    fn parse_modifiers_combined() {
        let mods = Modifier::parse_modifiers(Some("shift+alt"));
        assert_eq!(mods.len(), 2);
        assert!(mods.contains(&Modifier::Shift));
        assert!(mods.contains(&Modifier::Option));
    }

    #[test]
    fn parse_modifiers_unknown_skipped() {
        let mods = Modifier::parse_modifiers(Some("shift+banana+ctrl"));
        assert_eq!(mods.len(), 2);
    }

    #[test]
    fn click_options_defaults() {
        let opts = ClickOptions::default();
        assert_eq!(opts.button, MouseButton::Left);
        assert_eq!(opts.click_count, 1);
    }

    #[test]
    fn click_options_presets() {
        assert_eq!(ClickOptions::normal().button, MouseButton::Left);
        assert_eq!(ClickOptions::right_click().button, MouseButton::Right);
        assert_eq!(ClickOptions::double_click().click_count, 2);
    }

    #[test]
    fn drag_options_defaults() {
        let opts = DragOptions::default();
        assert_eq!(opts.steps, 30);
        assert!((opts.duration - 0.3).abs() < f64::EPSILON);
        assert!(opts.modifiers.is_empty());
        assert!(opts.pressure.is_none());
        assert!(!opts.right_button);
        assert!(!opts.close_path);
    }
}
