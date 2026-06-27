//! Element tree types: nodes, refs, and tree structure.
use std::fmt;

use crate::core::role::Role;
use crate::core::tree_pruning::{prune_node, PruningOptions};
use crate::core::types::Rect;

/// Where an element's accessible name was derived from.
///
/// Each platform backend resolves names through a fallback chain whose
/// priority order mirrors the W3C accessible name computation, and tags the
/// result with the *origin*. Consumers can distinguish author-provided names
/// from heuristically-derived ones — e.g. an accessibility audit can flag an
/// icon button whose name was inferred from a CSS icon-font class rather than
/// declared by the author.
///
/// For web content the browser has already run the W3C computation and placed
/// the result in the platform's title attribute, so this chain mostly fires
/// for native apps and sparse/incomplete trees where the title is absent.
///
/// The variants are normalized across platforms: `AXTitle` (macOS), UIA
/// `CurrentName` (Windows), and atspi `Name` (Linux) all map to [`Self::Title`].
/// `Some(source)` holds iff [`ElementData::name`] is `Some`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum NameSource {
    /// Direct title/name attribute (`AXTitle` / UIA `CurrentName` / atspi `Name`).
    Title,
    /// A description attribute used as the name (`AXDescription` / atspi
    /// `Description`) when no title was present.
    Description,
    /// A referenced title element's value or title (`AXTitleUIElement`).
    TitleUiElement,
    /// Label text from a child element (`AXStaticText` value / `AXImage` name /
    /// UIA/atspi first text child).
    ChildLabel,
    /// Help/tooltip text (`AXHelp` / UIA `CurrentHelpText` / atspi `HelpText`).
    HelpText,
    /// Placeholder value (`AXPlaceholderValue`).
    Placeholder,
    /// Heuristic name parsed from a DOM class-list icon font (`AXDOMClassList`,
    /// Electron icon buttons).
    IconClass,
    /// Role description used as a fallback name (`AXRoleDescription`).
    RoleDescription,
}

impl fmt::Display for NameSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Title => "title".fmt(f),
            Self::Description => "description".fmt(f),
            Self::TitleUiElement => "UI element title".fmt(f),
            Self::ChildLabel => "child label".fmt(f),
            Self::HelpText => "help text".fmt(f),
            Self::Placeholder => "placeholder".fmt(f),
            Self::IconClass => "icon class".fmt(f),
            Self::RoleDescription => "role description".fmt(f),
        }
    }
}

impl NameSource {
    /// Stable identifier matching the serialized (JSON) variant name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Description => "description",
            Self::TitleUiElement => "title_ui_element",
            Self::ChildLabel => "child_label",
            Self::HelpText => "help_text",
            Self::Placeholder => "placeholder",
            Self::IconClass => "icon_class",
            Self::RoleDescription => "role_description",
        }
    }
}

/// Per-element data, independent of tree structure.
///
/// Most consumers only need this -- children are expensive to walk and
/// often unnecessary (indexing, state comparison, audit rules).
#[derive(Debug, Clone, serde::Serialize)]
#[must_use]
#[non_exhaustive]
pub struct ElementData {
    /// The element's accessibility role.
    pub role: Role,
    /// The element's accessible name, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where [`name`](Self::name) was derived from. `Some` iff `name` is
    /// `Some`. Distinguishes author-provided names from heuristic fallbacks
    /// (icon-class parsing, role description). Shown only in verbose text output;
    /// always present in JSON when `name` is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_source: Option<NameSource>,
    /// The element's current value, if any (e.g. text field contents).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Interactive-element reference (e.g. "@e3"). None for structural elements.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<ElementRef>,
    /// Element bounds in screen coordinates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Rect>,
    /// Element bounds in window-relative coordinates (`bounds` minus the
    /// window origin). `None` when the node has no `bounds` or the snapshot
    /// has no `window_bounds`. Populated by [`ElementTree::enrich`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds_window: Option<Rect>,
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
    /// Stable identifier within a single snapshot tree.
    ///
    /// Assigned depth-first to every element during snapshot construction.
    /// Survives tree filtering (menu/offscreen toggles).
    /// Does NOT survive between snapshots — resets on every snapshot call.
    /// `None` for elements not going through `RefAssigner`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u64>,
    /// Content-based identity hash for cross-snapshot element matching.
    ///
    /// Hash of `(role, name?, identifier?, native_role?)` using FNV-1a 64-bit.
    /// Same content across two snapshots → same signature.
    /// Changed content → different signature.
    /// Undifferentiated elements (same role, unnamed, un-identified) →
    /// same signature.
    /// `None` for elements not going through `RefAssigner`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<u64>,
    /// Content + bounds-based identity hash for cross-snapshot matching.
    ///
    /// Hash of `(role, name?, identifier?, native_role?, bounds?)` — same as
    /// `signature` but with bounds folded in. Changes when the element moves
    /// or its content changes. Use when you need to disambiguate
    /// content-identical elements at different positions.
    /// `None` for elements not going through `RefAssigner`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_bounds: Option<u64>,
    /// Platform-specific attributes (key-value pairs for verbose output).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
}

impl ElementData {
    /// Create a new `ElementData` with the given role and default values.
    pub fn new(role: Role) -> Self {
        Self {
            role,
            name: None,
            name_source: None,
            value: None,
            reference: None,
            bounds: None,
            bounds_window: None,
            enabled: None,
            focused: None,
            selected: None,
            description: None,
            native_role: None,
            identifier: None,
            uid: None,
            signature: None,
            signature_bounds: None,
            attributes: Vec::new(),
        }
    }

    /// Set the accessible name and its derivation source (consumes `self`,
    /// builder pattern). Use [`with_resolved_name`](Self::with_resolved_name) for
    /// the conditional (`Option`) case.
    pub fn with_name(mut self, name: impl Into<String>, source: NameSource) -> Self {
        self.name = Some(name.into());
        self.name_source = Some(source);
        self
    }

    /// Set the name and its source from a resolved `(name, source)` pair
    /// (consumes `self`, builder pattern). `None` leaves both unset. Preserves
    /// the invariant that `name_source` is `Some` iff `name` is `Some`.
    pub fn with_resolved_name(mut self, resolved: Option<(String, NameSource)>) -> Self {
        if let Some((name, source)) = resolved {
            self.name = Some(name);
            self.name_source = Some(source);
        }
        self
    }

    /// Set the current value (consumes `self`, builder pattern).
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Set the element ref (consumes `self`, builder pattern).
    pub fn with_reference(mut self, reference: ElementRef) -> Self {
        self.reference = Some(reference);
        self
    }

    /// Set the element bounds (consumes `self`, builder pattern).
    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// Set the element bounds from an `Option` (consumes `self`, builder pattern).
    pub fn with_bounds_opt(mut self, bounds: Option<Rect>) -> Self {
        self.bounds = bounds;
        self
    }

    /// Set the enabled state (consumes `self`, builder pattern).
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Set the focused state (consumes `self`, builder pattern).
    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = Some(focused);
        self
    }

    /// Set the selected state (consumes `self`, builder pattern).
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Set the accessible description (consumes `self`, builder pattern).
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the native role string (consumes `self`, builder pattern).
    pub fn with_native_role(mut self, native_role: impl Into<String>) -> Self {
        self.native_role = Some(native_role.into());
        self
    }

    /// Set the platform identifier (consumes `self`, builder pattern).
    pub fn with_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }

    /// Add a platform-specific attribute (consumes `self`, builder pattern).
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
    /// Element data (role, name, value, bounds, etc.).
    pub data: ElementData,
    /// Child elements in the accessibility tree.
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

    /// Set the children (consumes `self`, builder pattern).
    pub fn with_children(mut self, children: Vec<Self>) -> Self {
        self.children = children;
        self
    }

    /// Add a child node.
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
#[non_exhaustive]
pub struct ElementTree {
    /// The application name this tree was captured from.
    pub app: String,
    /// Root node of the accessibility tree.
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
    /// Create a new tree for the given app with the given root.
    pub fn new(app: impl Into<String>, root: ElementNode) -> Self {
        Self {
            app: app.into(),
            root,
            refs: std::collections::HashMap::new(),
            window_bounds: None,
            timing: None,
        }
    }

    /// Store the ref map from this snapshot.
    pub fn with_references(
        mut self,
        refs: std::collections::HashMap<ElementRef, ElementRefInfo>,
    ) -> Self {
        self.refs = refs;
        self
    }

    /// Set the window bounds for this tree.
    pub fn with_window_bounds(mut self, bounds: Rect) -> Self {
        self.window_bounds = Some(bounds);
        self
    }

    /// Attach timing info to this tree.
    pub fn with_timing(mut self, timing: SnapshotTiming) -> Self {
        self.timing = Some(timing);
        self
    }

    /// Populate post-build derived fields across the tree.
    ///
    /// Currently computes each node's [`ElementData::bounds_window`] from its
    /// screen `bounds` minus [`Self::window_bounds`]. No-op when the tree has
    /// no `window_bounds`; nodes without `bounds` are skipped. Idempotent.
    ///
    /// Centralizes the screen→window coordinate shift so all consumers read a
    /// consistent window-relative value rather than each re-deriving the
    /// subtraction (a recurring source of off-by-origin bugs). Platforms call
    /// this once after assembling the tree; [`filter_tree`] re-runs it because
    /// pruning rebuilds nodes.
    pub fn enrich(&mut self) {
        fn walk(node: &mut ElementNode, window: Rect) {
            if let Some(b) = node.data.bounds {
                node.data.bounds_window = Some(b.translate(window));
            }
            for child in &mut node.children {
                walk(child, window);
            }
        }
        let Some(window) = self.window_bounds else {
            return;
        };
        walk(&mut self.root, window);
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
    /// Create new timing info.
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
    /// Unique numeric identifier within a snapshot.
    pub id: i32,
}

impl ElementRef {
    /// Create a new element ref with the given ID.
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
    /// The element's role.
    pub role: Role,
    /// The element's accessible name, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ElementRefInfo {
    /// Create a new `ElementRefInfo` with the given role and name.
    #[must_use]
    pub fn new(role: Role, name: Option<String>) -> Self {
        Self { role, name }
    }
}

/// Options for filtering an element tree.
#[derive(Clone, Debug, Default)]
pub struct FilterOptions {
    /// Exclude menu bar elements (and their subtrees).
    pub exclude_menu_bar: bool,
    /// Exclude elements outside the viewport.
    pub exclude_offscreen: bool,
}

/// Filter an element tree, removing menu bar and offscreen elements.
///
/// Returns a new `ElementTree` with the filtered root. Elements are removed if
/// they match the filter options — menu bar elements are excluded by role,
/// offscreen elements are excluded by bounds overlap with the viewport.
///
/// The viewport is used for offscreen detection. If `viewport` is `None`,
/// the tree's `window_bounds` are used if available, otherwise no offscreen
/// filtering is applied.
pub fn filter_tree(
    tree: &ElementTree,
    viewport: Option<Rect>,
    options: &FilterOptions,
) -> ElementTree {
    let vp = viewport.or(tree.window_bounds);
    let pruning = PruningOptions {
        exclude_menu_bar: options.exclude_menu_bar,
        exclude_offscreen: options.exclude_offscreen,
        skip_zero_size: false,
    };
    let root = prune_node(&tree.root, vp.as_ref(), 0, &pruning)
        .unwrap_or_else(|| ElementNode::new(ElementData::new(Role::Application)));
    let mut filtered = ElementTree {
        app: tree.app.clone(),
        root,
        refs: tree.refs.clone(),
        window_bounds: tree.window_bounds,
        timing: tree.timing.clone(),
    };
    // Pruning rebuilds nodes via `ElementData::new`, dropping `bounds_window`,
    // so recompute it for the filtered tree.
    filtered.enrich();
    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_is_interactive() {
        let button =
            ElementNode::new(ElementData::new(Role::Button).with_name("OK", NameSource::Title));
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

    #[test]
    fn enrich_populates_window_relative_bounds() {
        // Window at (520, 244); child at screen (532, 342) -> (12, 98) relative.
        let mut tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Window).with_bounds(Rect::new(520.0, 244.0, 760.0, 720.0)),
            )
            .with_children(vec![ElementNode::new(
                ElementData::new(Role::Button).with_bounds(Rect::new(532.0, 342.0, 736.0, 33.0)),
            )]),
        )
        .with_window_bounds(Rect::new(520.0, 244.0, 760.0, 720.0));

        tree.enrich();

        let child = &tree.root.children[0].data;
        let rel = child.bounds_window.expect("child bounds_window set");
        assert!((rel.x - 12.0).abs() < 1e-9);
        assert!((rel.y - 98.0).abs() < 1e-9);
        assert!((rel.width - 736.0).abs() < 1e-9);
        assert!((rel.height - 33.0).abs() < 1e-9);
        // Screen-absolute bounds are untouched.
        assert_eq!(child.bounds, Some(Rect::new(532.0, 342.0, 736.0, 33.0)));
    }

    #[test]
    fn enrich_is_noop_without_window_bounds() {
        let mut tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Button).with_bounds(Rect::new(10.0, 20.0, 5.0, 5.0)),
            ),
        );
        tree.enrich();
        assert!(tree.root.data.bounds_window.is_none());
    }

    #[test]
    fn enrich_skips_nodes_without_bounds() {
        let mut tree = ElementTree::new(
            "App",
            ElementNode::new(ElementData::new(Role::Group)) // no bounds
                .with_children(vec![ElementNode::new(
                    ElementData::new(Role::Button).with_bounds(Rect::new(532.0, 342.0, 10.0, 10.0)),
                )]),
        )
        .with_window_bounds(Rect::new(520.0, 244.0, 760.0, 720.0));

        tree.enrich();

        // Structural node without bounds stays None.
        assert!(tree.root.data.bounds_window.is_none());
        // Child with bounds is populated.
        assert!(tree.root.children[0].data.bounds_window.is_some());
    }

    #[test]
    fn enrich_is_idempotent() {
        let mk = || {
            ElementTree::new(
                "App",
                ElementNode::new(
                    ElementData::new(Role::Button).with_bounds(Rect::new(532.0, 342.0, 10.0, 10.0)),
                ),
            )
            .with_window_bounds(Rect::new(520.0, 244.0, 760.0, 720.0))
        };
        let mut once = mk();
        let mut twice = mk();
        once.enrich();
        twice.enrich();
        twice.enrich();
        assert_eq!(once.root.data.bounds_window, twice.root.data.bounds_window);
    }

    #[test]
    fn filter_tree_re_enriches_window_bounds() {
        // Parent with bounds; child has bounds and would survive a no-op filter.
        // After filter_tree, the rebuilt nodes must still carry bounds_window.
        let mut tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Window).with_bounds(Rect::new(520.0, 244.0, 760.0, 720.0)),
            )
            .with_children(vec![ElementNode::new(
                ElementData::new(Role::Button).with_bounds(Rect::new(532.0, 342.0, 10.0, 10.0)),
            )]),
        )
        .with_window_bounds(Rect::new(520.0, 244.0, 760.0, 720.0));
        tree.enrich();

        let filtered = filter_tree(
            &tree,
            None,
            &FilterOptions {
                exclude_menu_bar: false,
                exclude_offscreen: false,
            },
        );

        let child = &filtered.root.children[0].data;
        let rel = child.bounds_window.expect("filtered child bounds_window");
        assert!((rel.x - 12.0).abs() < 1e-9);
        assert!((rel.y - 98.0).abs() < 1e-9);
    }
}
