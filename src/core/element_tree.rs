/// Element tree types: nodes, refs, and tree structure.
use std::fmt;

use crate::core::role::Role;
use crate::core::types::Rect;

/// Per-element data, independent of tree structure.
///
/// Most consumers only need this -- children are expensive to walk and
/// often unnecessary (indexing, state comparison, audit rules).
#[derive(Debug, Clone, serde::Serialize)]
#[must_use]
pub struct ElementData {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<ElementRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Rect>,
    /// Whether the element is enabled (interactive). `None` if the platform
    /// doesn't report this state (e.g. structural containers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Whether the element currently has keyboard focus.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    /// Whether the element is selected (e.g. tab, list item, table row).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    /// Accessible description (tooltip text, help text, or detailed label).
    /// Distinct from `name` -- description provides additional context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The raw platform role string (e.g. `"AXButton"`, `"UIA 50000"`,
    /// `"ATSPI 28"`). Useful for debugging when a role maps to `Unknown`.
    /// Shown only in verbose text output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_role: Option<String>,
    /// Platform element identifier (`AXIdentifier` on macOS, `AutomationId` on
    /// Windows). Stable across launches -- useful for targeting elements.
    /// Shown only in verbose text output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
}

impl ElementData {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            name: None,
            value: None,
            reference: None,
            bounds: None,
            enabled: None,
            focused: None,
            selected: None,
            description: None,
            native_role: None,
            identifier: None,
            attributes: Vec::new(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_name_opt(mut self, name: Option<String>) -> Self {
        self.name = name;
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn with_reference(mut self, reference: ElementRef) -> Self {
        self.reference = Some(reference);
        self
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn with_bounds_opt(mut self, bounds: Option<Rect>) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = Some(focused);
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_native_role(mut self, native_role: impl Into<String>) -> Self {
        self.native_role = Some(native_role.into());
        self
    }

    pub fn with_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push((key.into(), value.into()));
        self
    }

    /// Whether this element is interactive (should receive a ref).
    #[must_use]
    pub fn is_interactive(&self) -> bool {
        self.role.is_interactive()
    }
}

/// A node in the accessibility element tree.
#[derive(Debug, Clone, serde::Serialize)]
#[must_use]
pub struct ElementNode {
    pub data: ElementData,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Self>,
}

impl ElementNode {
    /// Create a node from element data.
    pub fn new(data: ElementData) -> Self {
        Self {
            data,
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<Self>) -> Self {
        self.children = children;
        self
    }

    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }

    /// Whether this element is interactive (should receive a ref).
    #[must_use]
    pub fn is_interactive(&self) -> bool {
        self.data.is_interactive()
    }
}

/// The full accessibility tree for a window/app.
#[derive(Debug, Clone, serde::Serialize)]
#[must_use]
pub struct ElementTree {
    pub app: String,
    pub root: ElementNode,
    /// All refs assigned in this snapshot, in order.
    #[serde(skip_serializing)]
    pub refs: std::collections::HashMap<ElementRef, ElementRefInfo>,
    /// Window bounds in screen coordinates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_bounds: Option<Rect>,
    /// Performance timing breakdown.
    #[serde(skip_serializing)]
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

    pub fn with_references(
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
    #[must_use]
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
    #[must_use]
    pub fn report(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        #[expect(
            clippy::cast_precision_loss,
            reason = "node count fits in f64 mantissa"
        )]
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
        Self::append_subtree_report(&self.root, 0, self.node_count, threshold, &mut lines);
        lines.join("\n")
    }

    fn append_subtree_report(
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
            if large_children.len() == 1 && child.data.name.is_none() {
                Self::append_subtree_report(child, indent, total, threshold, lines);
                continue;
            }

            #[expect(
                clippy::cast_precision_loss,
                reason = "node count fits in f64 mantissa"
            )]
            let pct = if total > 0 {
                count as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            let label = Self::node_label(child);
            let prefix = "  ".repeat(indent + 1);
            lines.push(format!("{prefix}{label} {count:5} nodes  {pct:5.1}%"));

            if count >= threshold && !child.children.is_empty() {
                Self::append_subtree_report(child, indent + 1, total, threshold, lines);
            }
        }
    }

    fn node_label(node: &ElementNode) -> String {
        let name =
            node.data
                .name
                .as_ref()
                .and_then(|n| if n.is_empty() { None } else { Some(n.as_str()) });
        let label = name.map_or_else(
            || node.data.role.short_name().to_owned(),
            |n| format!("{} \"{}\"", node.data.role.short_name(), n),
        );
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
    #[must_use]
    pub fn new(id: i32) -> Self {
        Self { id }
    }

    /// Parse a ref string like "@e3" into an `ElementRef`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if !trimmed.starts_with("@e") {
            return None;
        }
        let id: i32 = trimmed.strip_prefix("@e")?.parse().ok()?;
        Some(Self::new(id))
    }
}

impl serde::Serialize for ElementRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl fmt::Display for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@e{}", self.id)
    }
}

/// Info stored alongside a ref for action dispatch.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ElementRefInfo {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ElementRefInfo {
    #[must_use]
    pub fn new(role: Role, name: Option<String>) -> Self {
        Self { role, name }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_is_interactive() {
        let button = ElementNode::new(ElementData::new(Role::Button).with_name("OK"));
        let group = ElementNode::new(ElementData::new(Role::Group));
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
