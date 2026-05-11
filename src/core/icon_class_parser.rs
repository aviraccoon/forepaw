/// Extracts semantic icon names from CSS class lists.
///
/// Electron apps use icon libraries (Lucide, Tabler, FontAwesome, etc.)
/// whose class names encode the icon identity. This parser strips known
/// prefixes to extract the semantic name.
///
/// [`IconClassParser`]
pub struct IconClassParser;

impl IconClassParser {
    pub fn new() -> Self {
        Self
    }

    /// Extract a semantic icon name from a list of CSS classes.
    pub fn parse(&self, classes: &[&str]) -> Option<String> {
        for cls in classes {
            if let Some(name) = self.extract_icon_name(cls) {
                return Some(name);
            }
        }
        None
    }

    fn extract_icon_name(&self, cls: &str) -> Option<String> {
        // Skip generic/non-semantic classes
        if GENERIC_CLASSES.contains(&cls) {
            return None;
        }

        // Try each prefix pattern (order matters -- longer/more specific first)
        for prefix in PREFIXES {
            if let Some(stripped) = cls.strip_prefix(prefix) {
                if !stripped.is_empty() && stripped != cls {
                    return self.sanitize(stripped);
                }
            }
        }

        None
    }

    fn sanitize(&self, name: &str) -> Option<String> {
        let cleaned: String = name.replace('-', " ").trim().to_string();
        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned)
        }
    }
}

impl Default for IconClassParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Prefixes that precede the semantic icon name.
/// Order matters -- longer/more specific prefixes first.
const PREFIXES: &[&str] = &[
    "lucide-",
    "icon-tabler-",
    "fa-solid fa-",
    "fa-regular fa-",
    "fa-brands fa-",
    "fa-",
    "mdi-",
    "material-symbols-",
    "material-icons-",
    "heroicon-",
    "hero-",
    "ph-",
    "ri-",
    "bi-",
    "feather-",
    "ion-",
    "octicon-",
    "codicon-",
    "icon-",
];

/// Classes that are generic wrappers, not semantic names.
const GENERIC_CLASSES: &[&str] = &[
    "icon",
    "icons",
    "svg-icon",
    "svg",
    "img",
    "fa",
    "fas",
    "far",
    "fab",
    "fal",
    "fad",
    "material-icons",
    "material-symbols",
    "bi",
    "ri",
    "ph",
    "icon-tabler",
    "icons-tabler-outline",
    "icons-tabler-filled",
    "chevron-icon",
    "section-icon",
    "flex-shrink-0",
    "p-1",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lucide_settings() {
        assert_eq!(
            IconClassParser::new().parse(&["svg-icon", "lucide-settings"]),
            Some("settings".into())
        );
    }

    #[test]
    fn lucide_multi_word() {
        assert_eq!(
            IconClassParser::new().parse(&["svg-icon", "lucide-file-search"]),
            Some("file search".into())
        );
    }

    #[test]
    fn tabler_home() {
        assert_eq!(
            IconClassParser::new().parse(&["icon", "icon-tabler", "icon-tabler-home"]),
            Some("home".into())
        );
    }

    #[test]
    fn tabler_with_outline_variant() {
        assert_eq!(
            IconClassParser::new().parse(&[
                "icon",
                "icon-tabler",
                "icons-tabler-outline",
                "icon-tabler-layout-sidebar"
            ]),
            Some("layout sidebar".into())
        );
    }

    #[test]
    fn font_awesome_search() {
        assert_eq!(
            IconClassParser::new().parse(&["fa", "fa-search"]),
            Some("search".into())
        );
    }

    #[test]
    fn font_awesome_brands() {
        assert_eq!(
            IconClassParser::new().parse(&["fab", "fa-github"]),
            Some("github".into())
        );
    }

    #[test]
    fn material_design() {
        assert_eq!(
            IconClassParser::new().parse(&["mdi", "mdi-account-circle"]),
            Some("account circle".into())
        );
    }

    #[test]
    fn codicon() {
        assert_eq!(
            IconClassParser::new().parse(&["codicon", "codicon-gear"]),
            Some("gear".into())
        );
    }

    #[test]
    fn bootstrap() {
        assert_eq!(
            IconClassParser::new().parse(&["bi", "bi-gear-fill"]),
            Some("gear fill".into())
        );
    }

    #[test]
    fn heroicon() {
        assert_eq!(
            IconClassParser::new().parse(&["hero-home-solid"]),
            Some("home solid".into())
        );
    }

    #[test]
    fn remix() {
        assert_eq!(
            IconClassParser::new().parse(&["ri-home-line"]),
            Some("home line".into())
        );
    }

    #[test]
    fn phosphor() {
        assert_eq!(
            IconClassParser::new().parse(&["ph-gear-six"]),
            Some("gear six".into())
        );
    }

    #[test]
    fn feather() {
        assert_eq!(
            IconClassParser::new().parse(&["feather-arrow-left"]),
            Some("arrow left".into())
        );
    }

    #[test]
    fn empty_class_list() {
        assert!(IconClassParser::new().parse(&[]).is_none());
    }

    #[test]
    fn only_generic_classes() {
        assert!(IconClassParser::new()
            .parse(&["icon", "svg-icon"])
            .is_none());
    }

    #[test]
    fn utility_classes_only() {
        assert!(IconClassParser::new()
            .parse(&["flex-shrink-0", "p-1"])
            .is_none());
    }

    #[test]
    fn no_recognized_prefix() {
        assert!(IconClassParser::new()
            .parse(&["custom-widget", "my-component"])
            .is_none());
    }

    #[test]
    fn hashed_class_names() {
        assert!(IconClassParser::new().parse(&["canvas_eb6eba"]).is_none());
    }

    #[test]
    fn first_icon_class_wins() {
        assert_eq!(
            IconClassParser::new().parse(&["lucide-home", "lucide-settings"]),
            Some("home".into())
        );
    }

    #[test]
    fn obsidian_panel_left() {
        assert_eq!(
            IconClassParser::new().parse(&["svg-icon", "lucide-panel-left"]),
            Some("panel left".into())
        );
    }
}
