/// Element tree types: nodes, refs, and tree structure.
use std::fmt;

use crate::core::types::Rect;

/// Roles that are considered interactive and should receive refs.
pub const INTERACTIVE_ROLES: &[&str] = &[
    "AXButton",
    "AXTextField",
    "AXTextArea",
    "AXCheckBox",
    "AXRadioButton",
    "AXSlider",
    "AXComboBox",
    "AXPopUpButton",
    "AXMenuButton",
    "AXLink",
    "AXMenuItem",
    "AXTab",
    "AXSwitch",
    "AXIncrementor",
    "AXColorWell",
    "AXTreeItem",
    "AXCell",
    "AXDockItem",
];

/// Check if a role is interactive (should receive a ref).
pub fn is_interactive_role(role: &str) -> bool {
    INTERACTIVE_ROLES.contains(&role)
}

/// A node in the accessibility element tree.
#[derive(Debug, Clone)]
pub struct ElementNode {
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub r#ref: Option<ElementRef>,
    pub bounds: Option<Rect>,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<ElementNode>,
}

impl ElementNode {
    pub fn new(role: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            name: None,
            value: None,
            r#ref: None,
            bounds: None,
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn with_ref(mut self, r#ref: ElementRef) -> Self {
        self.r#ref = Some(r#ref);
        self
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push((key.into(), value.into()));
        self
    }

    pub fn with_children(mut self, children: Vec<ElementNode>) -> Self {
        self.children = children;
        self
    }

    pub fn add_child(&mut self, child: ElementNode) {
        self.children.push(child);
    }

    /// Whether this element is interactive (should receive a ref).
    pub fn is_interactive(&self) -> bool {
        is_interactive_role(&self.role)
    }
}

/// The full accessibility tree for a window/app.
#[derive(Debug, Clone)]
pub struct ElementTree {
    pub app: String,
    pub root: ElementNode,
    /// All refs assigned in this snapshot, in order.
    pub refs: std::collections::HashMap<ElementRef, ElementRefInfo>,
    /// Window bounds in screen coordinates.
    pub window_bounds: Option<Rect>,
    /// Performance timing breakdown.
    pub timing: Option<SnapshotTiming>,
}

impl ElementTree {
    pub fn new(app: impl Into<String>, root: ElementNode) -> Self {
        Self {
            app: app.into(),
            root,
            refs: std::collections::HashMap::new(),
            window_bounds: None,
            timing: None,
        }
    }

    pub fn with_refs(
        mut self,
        refs: std::collections::HashMap<ElementRef, ElementRefInfo>,
    ) -> Self {
        self.refs = refs;
        self
    }

    pub fn with_window_bounds(mut self, bounds: Rect) -> Self {
        self.window_bounds = Some(bounds);
        self
    }

    pub fn with_timing(mut self, timing: SnapshotTiming) -> Self {
        self.timing = Some(timing);
        self
    }
}

/// Performance timing for a snapshot.
#[derive(Debug, Clone)]
pub struct SnapshotTiming {
    /// Total wall time for the tree walk in milliseconds.
    pub total_ms: f64,
    /// Total number of nodes visited.
    pub node_count: usize,
    /// The root of the tree (for adaptive breakdown).
    pub root: ElementNode,
}

impl SnapshotTiming {
    pub fn new(total_ms: f64, node_count: usize, root: ElementNode) -> Self {
        Self {
            total_ms,
            node_count,
            root,
        }
    }

    /// Count total nodes in a subtree.
    pub fn count_nodes(node: &ElementNode) -> usize {
        1 + node.children.iter().map(Self::count_nodes).sum::<usize>()
    }

    /// Format timing as a human-readable report.
    pub fn report(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        let avg = if self.node_count > 0 {
            self.total_ms / self.node_count as f64
        } else {
            0.0
        };
        lines.push(format!(
            "snapshot: {:.0}ms, {} nodes, {:.1}ms/node avg",
            self.total_ms, self.node_count, avg
        ));
        let threshold = std::cmp::max(self.node_count / 10, 2);
        self.append_subtree_report(&self.root, 0, self.node_count, threshold, &mut lines);
        lines.join("\n")
    }

    fn append_subtree_report(
        &self,
        node: &ElementNode,
        indent: usize,
        total: usize,
        threshold: usize,
        lines: &mut Vec<String>,
    ) {
        for child in &node.children {
            let count = Self::count_nodes(child);
            if count < threshold {
                continue;
            }

            // Skip single-child chains: if this node has exactly one large child,
            // don't print this node -- just recurse into the child.
            let large_children: Vec<_> = child
                .children
                .iter()
                .filter(|c| Self::count_nodes(c) >= threshold)
                .collect();
            if large_children.len() == 1 && child.name.is_none() {
                self.append_subtree_report(child, indent, total, threshold, lines);
                continue;
            }

            let pct = if total > 0 {
                count as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            let label = Self::node_label(child);
            let prefix = "  ".repeat(indent + 1);
            lines.push(format!("{prefix}{label} {count:5} nodes  {pct:5.1}%"));

            if count >= threshold && !child.children.is_empty() {
                self.append_subtree_report(child, indent + 1, total, threshold, lines);
            }
        }
    }

    fn node_label(node: &ElementNode) -> String {
        let name = node
            .name
            .as_ref()
            .and_then(|n| if n.is_empty() { None } else { Some(n.as_str()) });
        let label = name
            .map(|n| format!("{} \"{}\"", node.role, n))
            .unwrap_or_else(|| node.role.clone());
        let truncated: String = label.chars().take(40).collect();
        truncated
    }
}

/// Opaque reference to an interactive element, valid until the next snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ElementRef {
    pub id: i32,
}

impl ElementRef {
    pub fn new(id: i32) -> Self {
        Self { id }
    }

    /// Parse a ref string like "@e3" into an ElementRef.
    pub fn parse(s: &str) -> Option<ElementRef> {
        let trimmed = s.trim();
        if !trimmed.starts_with("@e") {
            return None;
        }
        let id: i32 = trimmed[2..].parse().ok()?;
        Some(ElementRef::new(id))
    }
}

impl fmt::Display for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@e{}", self.id)
    }
}

/// Info stored alongside a ref for action dispatch.
#[derive(Debug, Clone)]
pub struct ElementRefInfo {
    pub role: String,
    pub name: Option<String>,
}

impl ElementRefInfo {
    pub fn new(role: impl Into<String>, name: Option<String>) -> Self {
        Self {
            role: role.into(),
            name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_roles_identified() {
        assert!(is_interactive_role("AXButton"));
        assert!(is_interactive_role("AXTextField"));
        assert!(is_interactive_role("AXCheckBox"));
        assert!(is_interactive_role("AXLink"));
        assert!(is_interactive_role("AXMenuItem"));
        assert!(is_interactive_role("AXSlider"));
        assert!(is_interactive_role("AXPopUpButton"));
        assert!(is_interactive_role("AXSwitch"));
    }

    #[test]
    fn non_interactive_roles_identified() {
        assert!(!is_interactive_role("AXGroup"));
        assert!(!is_interactive_role("AXWindow"));
        assert!(!is_interactive_role("AXStaticText"));
        assert!(!is_interactive_role("AXImage"));
        assert!(!is_interactive_role("AXScrollArea"));
        assert!(!is_interactive_role("AXUnknown"));
        assert!(!is_interactive_role(""));
    }

    #[test]
    fn node_is_interactive() {
        let button = ElementNode::new("AXButton").with_name("OK");
        let group = ElementNode::new("AXGroup");
        assert!(button.is_interactive());
        assert!(!group.is_interactive());
    }

    #[test]
    fn ref_display_format() {
        assert_eq!(ElementRef::new(1).to_string(), "@e1");
        assert_eq!(ElementRef::new(42).to_string(), "@e42");
        assert_eq!(ElementRef::new(100).to_string(), "@e100");
    }

    #[test]
    fn ref_parse_roundtrip() {
        for id in [1, 5, 42, 100, 999] {
            let r = ElementRef::new(id);
            let parsed = ElementRef::parse(&r.to_string());
            assert_eq!(parsed, Some(r));
        }
    }

    #[test]
    fn ref_parse_edge_cases() {
        assert_eq!(ElementRef::parse("@e0"), Some(ElementRef::new(0)));
        assert_eq!(ElementRef::parse("  @e5  "), Some(ElementRef::new(5)));
        assert_eq!(ElementRef::parse("@e"), None);
        assert_eq!(ElementRef::parse("@"), None);
        assert_eq!(ElementRef::parse(""), None);
        assert_eq!(ElementRef::parse("@eabc"), None);
    }
}
