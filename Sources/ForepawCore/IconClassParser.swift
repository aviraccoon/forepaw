/// Extracts semantic icon names from CSS class lists.
///
/// Electron apps (Obsidian, Bruno, VS Code, etc.) use icon libraries like
/// Lucide, Tabler Icons, and FontAwesome. Their unnamed AXImage elements
/// expose class lists via AXDOMClassList that contain the icon identity:
///
///     ["svg-icon", "lucide-settings"]     -> "settings"
///     ["icon", "icon-tabler", "icon-tabler-home"] -> "home"
///     ["fa", "fa-search"]                 -> "search"
///
/// This parser strips known prefixes to extract the semantic name.
public struct IconClassParser: Sendable {
    public init() {}

    /// Extract a semantic icon name from a list of CSS classes.
    /// Returns nil if no icon class is recognized.
    public func parse(_ classes: [String]) -> String? {
        for cls in classes {
            if let name = extractIconName(cls) {
                return name
            }
        }
        return nil
    }

    /// Extract icon name from a single CSS class string.
    private func extractIconName(_ cls: String) -> String? {
        // Skip generic/non-semantic classes
        if genericClasses.contains(cls) { return nil }

        // Try each prefix pattern
        for prefix in prefixes {
            if cls.hasPrefix(prefix) {
                let remainder = String(cls.dropFirst(prefix.count))
                if !remainder.isEmpty && remainder != cls {
                    return sanitize(remainder)
                }
            }
        }

        return nil
    }

    /// Clean up extracted name: convert kebab-case to spaces, trim.
    private func sanitize(_ name: String) -> String? {
        let cleaned =
            name
            .replacingOccurrences(of: "-", with: " ")
            .trimmingCharacters(in: .whitespaces)
        return cleaned.isEmpty ? nil : cleaned
    }

    // MARK: - Known patterns

    /// Prefixes that precede the semantic icon name.
    /// Order matters -- longer/more specific prefixes first.
    private let prefixes: [String] = [
        // Lucide Icons (used by Obsidian)
        "lucide-",
        // Tabler Icons (used by Bruno)
        "icon-tabler-",
        // FontAwesome
        "fa-solid fa-",
        "fa-regular fa-",
        "fa-brands fa-",
        "fa-",
        // Material Design Icons
        "mdi-",
        // Material Symbols
        "material-symbols-",
        "material-icons-",
        // Heroicons
        "heroicon-",
        "hero-",
        // Phosphor Icons
        "ph-",
        // Remix Icons
        "ri-",
        // Bootstrap Icons
        "bi-",
        // Feather Icons
        "feather-",
        // Ionicons
        "ion-",
        // Octicons (GitHub)
        "octicon-",
        // Codicons (VS Code)
        "codicon-",
        // Generic icon- prefix (catches icon-tabler without specific match)
        "icon-",
    ]

    /// Classes that are generic wrappers, not semantic names.
    private let genericClasses: Set<String> = [
        "icon", "icons", "svg-icon", "svg", "img",
        "fa", "fas", "far", "fab", "fal", "fad",
        "material-icons", "material-symbols",
        "bi", "ri", "ph",
        "icon-tabler", "icons-tabler-outline", "icons-tabler-filled",
        "chevron-icon", "section-icon",  // Obsidian/Bruno generic
        "flex-shrink-0", "p-1",  // utility classes
    ]
}
