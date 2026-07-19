//! AT-SPI2 tree snapshot: walk the accessibility tree and build an `ElementTree`.
//!
//! Connects to the AT-SPI2 bus, finds the target application, and walks its
//! accessible tree using `GetChildren` + property reads via D-Bus.
//! Maps AT-SPI2 role constants to `Role` variants so the core
//! ref assigner and tree renderer work unchanged across platforms.

use zbus::blocking::Connection;

use std::collections::{HashMap, HashSet};

use crate::core::element_tree::{
    ElementData, ElementNode, ElementRef, ElementTree, NameSource, SnapshotTiming,
};
use crate::core::errors::ForepawError;
use crate::core::ref_assigner::RefAssigner;
use crate::core::ref_cache::build_ref_handle_map;
use crate::core::role::Role;
use crate::core::types::Rect;
use crate::platform::{AppTarget, SnapshotOptions, WindowTarget};

use super::app::{
    connect_atspi_bus, find_app_bus, find_child_window, find_main_window_bounds, get_bounds,
    get_children, get_property, get_role, get_value,
};
use super::role::atspi_role_to_role;
use super::state::{
    STATE_CHECKABLE, STATE_CHECKED, STATE_COLLAPSED, STATE_EDITABLE, STATE_ENABLED, STATE_EXPANDED,
    STATE_FOCUSED, STATE_HAS_POPUP, STATE_INDETERMINATE, STATE_IS_DEFAULT, STATE_MODAL,
    STATE_PRESSED, STATE_READ_ONLY, STATE_SELECTABLE, STATE_SELECTED, STATE_SHOWING,
};

// AT-SPI2 role mapping is generated from res/atspi-constants.h.
// See src/platform/linux/role.rs.

// ---------------------------------------------------------------------------
// Ref → handle cache
// ---------------------------------------------------------------------------

/// The `(D-Bus bus name, object path)` identifying an AT-SPI2 accessible
/// element — the handle type for the ref→handle cache. Cheap to clone (two
/// strings); `Send + Sync` natively, so the cache needs no `unsafe impl Send`
/// (unlike macOS' `AXUIElementRef` and Windows' `IUIAutomationElement`).
#[derive(Debug, Clone)]
pub(super) struct AtspiRef {
    /// D-Bus bus name owning the element (the app's bus, or a per-child bus for Qt apps).
    pub(super) bus: String,
    /// Object path of the accessible element.
    pub(super) path: String,
}

/// Linux's parallel handle tree: the generic `core::ref_cache::HandleNode`
/// carrying an `AtspiRef` per node. Built in lockstep with `ElementNode`.
type HandleNode = crate::core::ref_cache::HandleNode<AtspiRef>;

/// Map from ref id to the retained `AtspiRef`, captured during the snapshot
/// walk. Stored on `LinuxProvider` for O(1) ref resolution.
#[derive(Debug, Default)]
pub(super) struct RefHandleMap(HashMap<i32, AtspiRef>);

impl RefHandleMap {
    /// Create an empty map.
    pub(super) fn empty() -> Self {
        Self(HashMap::new())
    }

    /// Look up the retained handle for a ref (cloned — `AtspiRef` is cheap).
    pub(super) fn get(&self, ref_id: i32) -> Option<AtspiRef> {
        self.0.get(&ref_id).cloned()
    }
}

// ---------------------------------------------------------------------------
// Name resolution helpers
// ---------------------------------------------------------------------------

/// Get the help text property for name resolution.
fn get_help_text(conn: &Connection, destination: &str, path: &str) -> Option<String> {
    crate::trace!("ENTER get_help_text {path}");
    get_property(conn, destination, path, "HelpText")
}

// ---------------------------------------------------------------------------
// Tree pruning
// ---------------------------------------------------------------------------

struct TreePruning {
    skip_zero_size: bool,
    skip_offscreen: bool,
    skip_menu_bar: bool,
    window_bounds: Option<Rect>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Walk the AT-SPI2 tree for the given app and return an `ElementTree`.
///
/// If `window` is specified, the tree walk is scoped to that specific window
/// (child frame/window) instead of the full app tree.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::WindowNotFound`] if the specified window is not found,
/// or [`ForepawError::ActionFailed`] if the AT-SPI2 bus is unreachable.
pub(super) fn snapshot(
    app: &AppTarget,
    window: Option<&WindowTarget>,
    options: &SnapshotOptions,
) -> Result<(ElementTree, RefHandleMap), ForepawError> {
    let conn = connect_atspi_bus()?;

    // Find the target app's bus name.
    let app_bus = find_app_bus(&conn, app)?;

    // If a window target is specified, resolve it to a child frame/window path.
    let (root_path, window_bounds): (String, Option<Rect>) = if let Some(target) = window {
        find_child_window(&conn, &app_bus, target)?
    } else {
        // No explicit window target: derive the app's main window origin so
        // coordinates come out window-relative (matching macOS/Windows).
        // An explicit `options.window_bounds` override wins if set.
        (
            "/org/a11y/atspi/accessible/root".to_owned(),
            options
                .window_bounds
                .or_else(|| find_main_window_bounds(&conn, &app_bus)),
        )
    };

    let pruning = TreePruning {
        skip_zero_size: options.skip_zero_size,
        skip_offscreen: options.skip_offscreen,
        skip_menu_bar: options.skip_menu_bar,
        window_bounds,
    };

    let walk_start = std::time::Instant::now();
    let (root, handle_root) =
        build_tree(&conn, &app_bus, &root_path, 0, options.max_depth, &pruning);
    let walk_ms = walk_start.elapsed().as_secs_f64() * 1000.0;

    let assigner = RefAssigner::new();
    let result = assigner.assign(&root, options.interactive_only);

    // Capture ref→handle from the same walk that produced the tree (interactive
    // nodes only, mirroring `RefAssigner`), so resolve calls are O(1).
    let ref_handles = RefHandleMap(build_ref_handle_map(&root, &handle_root));

    let timing = if options.timing {
        let node_count = SnapshotTiming::count_nodes(&result.root);
        Some(SnapshotTiming::new(
            walk_ms,
            node_count,
            result.root.clone(),
        ))
    } else {
        None
    };

    let mut tree = ElementTree {
        app: app.display(),
        root: result.root,
        refs: result.refs,
        window_bounds,
        timing,
    };
    tree.enrich();
    Ok((tree, ref_handles))
}

// ---------------------------------------------------------------------------
// Tree walk
// ---------------------------------------------------------------------------

#[expect(
    clippy::too_many_lines,
    reason = "recursive tree-walk; hard to shrink without obscuring the per-node flow"
)]
fn build_tree(
    conn: &Connection,
    app_bus: &str,
    path: &str,
    depth: usize,
    max_depth: usize,
    pruning: &TreePruning,
) -> (ElementNode, HandleNode) {
    if depth >= max_depth {
        return (
            ElementNode::new(ElementData::new(Role::Group)),
            HandleNode::default(),
        );
    }

    let role_num = get_role(conn, app_bus, path);
    let role = atspi_role_to_role(role_num);

    // Skip menu bar subtree when requested (matches Darwin; the menu items are
    // addressable via keyboard shortcuts or after opening the menu, so walking
    // a closed menu bar only adds noise — and on Qt 6.x it stresses a fragile
    // atspi bridge that can SIGSEGV in QSortFilterProxyModel::parent).
    if pruning.skip_menu_bar && role == Role::MenuBar {
        return (
            ElementNode::new(ElementData::new(role)),
            HandleNode::default(),
        );
    }

    crate::trace!("name   d={depth} {path}");
    // Get properties
    let name = get_property(conn, app_bus, path, "Name");
    crate::trace!("value  d={depth} {path}");
    let value = get_value(conn, app_bus, path);
    crate::trace!("bounds d={depth} {path}");
    let bounds = get_bounds(conn, app_bus, path);
    crate::trace!("state_set d={depth} {path}");
    let state_set = get_state_set(conn, app_bus, path).unwrap_or_default();
    let is_showing = state_set.contains(&STATE_SHOWING);

    // Handle for this node: the (bus, path) needed to address it for actions.
    let atspi_ref = AtspiRef {
        bus: app_bus.to_owned(),
        path: path.to_owned(),
    };

    // Check pruning conditions (zero-size and offscreen). A pruned interactive
    // leaf keeps its role (so it still gets a ref) and carries its handle,
    // mirroring `RefAssigner`.
    if let Some(pruned) = check_pruned(role, name.as_ref(), bounds.as_ref(), depth, pruning) {
        return (pruned, HandleNode::leaf(atspi_ref));
    }

    // Skip descent into non-showing containers. Collapsed toolviews, hidden
    // background-tab content, and closed-menu subtrees are all !SHOWING and
    // not actionable until opened — and on Qt 6.x, descending into their
    // model-backed widgets (QSortFilterProxyModel) can SIGSEGV the atspi
    // bridge when the app is in a fragile state (e.g. Kate post-close-tab).
    // The element itself stays (visible parent or interactive leaf keeps its
    // ref); only its children aren't walked.
    if !is_showing && depth > 1 {
        return (
            ElementNode::new(
                ElementData::new(role)
                    .with_resolved_name(
                        name.clone()
                            .filter(|s| !s.is_empty())
                            .map(|n| (n, NameSource::Title)),
                    )
                    .with_bounds_opt(bounds),
            ),
            HandleNode::leaf(atspi_ref),
        );
    }

    // Build children first (so name resolution can use them), with parallel
    // handle nodes in lockstep.
    crate::trace!("children d={depth} {path}");
    let children_paths = get_children(conn, app_bus, path).unwrap_or_default();
    let (children, child_handles): (Vec<ElementNode>, Vec<HandleNode>) = children_paths
        .iter()
        .map(|(child_bus, child_path)| {
            // Qt apps use per-app bus names; GTK apps may share the registry bus.
            let bus = if child_bus.starts_with(':') && child_bus != app_bus {
                child_bus.clone()
            } else {
                app_bus.to_owned()
            };
            build_tree(
                conn,
                &bus,
                child_path.as_str(),
                depth + 1,
                max_depth,
                pruning,
            )
        })
        .filter(|(node, _)| {
            // Drop interactive elements with no bounds (0×0 / no Component).
            // Qt's AT-SPI2 bridge exposes hidden duplicate toolbars (e.g. Kate
            // surfaces a non-showing "New"/"Open"/"Save" set alongside the
            // visible Main Toolbar); the hidden set reports 0×0 and isn't
            // coordinate-actionable. Dropping it lets the visible duplicates
            // take the refs. Non-interactive containers are kept (they may
            // have showing children), and closed-menu items — which report
            // non-zero offscreen bounds — are kept too.
            !(node.data.role.is_interactive() && node.data.bounds.is_none())
        })
        .unzip();

    // Name resolution: Name → Description → HelpText → first text child
    crate::trace!("resolve_name d={depth} {path}");
    let (final_name, name_source) =
        match resolve_name(conn, app_bus, path, name.as_ref(), &children) {
            Some((n, s)) => (Some(n), Some(s)),
            None => (None, None),
        };

    // Collect attributes (state, interfaces, etc.)
    let mut attributes: Vec<(String, String)> = Vec::new();
    let state = format_state(&state_set);
    if !state.is_empty() {
        attributes.push(("state".to_owned(), state));
    }

    let node = ElementNode {
        data: ElementData {
            role,
            name: final_name,
            name_source,
            value,
            reference: None,
            bounds,
            bounds_window: None,
            // TODO: populate from AT-SPI2 StateSet (ENABLED, FOCUSED, SELECTED) and Description
            enabled: None,
            focused: None,
            selected: None,
            description: None,
            native_role: None,
            identifier: None,
            uid: None,
            signature: None,
            signature_bounds: None,
            attributes,
        },
        children,
    };
    (
        node,
        HandleNode {
            handle: Some(atspi_ref),
            children: child_handles,
        },
    )
}

/// Whether `bounds` lies entirely outside `window` (no overlap). Shared by
/// `check_pruned` (snapshot) and `collect_atspi_refs` (rewalk) so the two walks
/// agree on what counts as offscreen — the rewalk must leaf-prune the same
/// subtrees the snapshot prunes, or ref numbering desyncs.
fn is_offscreen(bounds: &Rect, window: &Rect) -> bool {
    let no_horizontal = bounds.x + bounds.width <= window.x || bounds.x >= window.x + window.width;
    let no_vertical = bounds.y + bounds.height <= window.y || bounds.y >= window.y + window.height;
    no_horizontal || no_vertical
}

/// Check if this element should be pruned (zero-size or offscreen).
/// Returns `Some(pruned_node)` if the element should be skipped.
fn check_pruned(
    role: Role,
    name: Option<&String>,
    bounds: Option<&Rect>,
    depth: usize,
    pruning: &TreePruning,
) -> Option<ElementNode> {
    // Prune zero-size subtrees
    if pruning.skip_zero_size {
        if let Some(b) = bounds {
            if b.width == 0.0 && b.height == 0.0 && depth > 1 {
                return Some(ElementNode::new(
                    ElementData::new(role)
                        .with_resolved_name(
                            name.cloned()
                                .filter(|s| !s.is_empty())
                                .map(|n| (n, NameSource::Title)),
                        )
                        .with_bounds(*b),
                ));
            }
        }
    }

    // Prune offscreen elements
    if pruning.skip_offscreen && depth > 1 {
        if let (Some(wb), Some(b)) = (&pruning.window_bounds, bounds) {
            if is_offscreen(b, wb) {
                return Some(ElementNode::new(
                    ElementData::new(role)
                        .with_resolved_name(
                            name.cloned()
                                .filter(|s| !s.is_empty())
                                .map(|n| (n, NameSource::Title)),
                        )
                        .with_bounds(*b),
                ));
            }
        }
    }

    None
}

/// Resolve the accessible name, tagging the source.
///
/// Chain: atspi `Name` -> [`NameSource::Title`], `Description` ->
/// [`NameSource::Description`], `HelpText` -> [`NameSource::HelpText`],
/// first `StaticText` child's name -> [`NameSource::ChildLabel`].
fn resolve_name(
    conn: &Connection,
    app_bus: &str,
    path: &str,
    name: Option<&String>,
    children: &[ElementNode],
) -> Option<(String, NameSource)> {
    // 1. Name
    if let Some(n) = name.filter(|s| !s.is_empty()) {
        return Some((n.clone(), NameSource::Title));
    }

    // 2. Description
    crate::trace!("name:description {path}");
    if let Some(desc) = get_property(conn, app_bus, path, "Description").filter(|s| !s.is_empty()) {
        return Some((desc, NameSource::Description));
    }

    // 3. HelpText
    crate::trace!("name:helptext {path}");
    if let Some(help) = get_help_text(conn, app_bus, path).filter(|s| !s.is_empty()) {
        return Some((help, NameSource::HelpText));
    }

    // 4. First text child that looks like a label.
    if let Some(child_name) = first_text_child_name(children) {
        return Some((child_name, NameSource::ChildLabel));
    }

    None
}

/// Get the first child that looks like a text label.
fn first_text_child_name(children: &[ElementNode]) -> Option<String> {
    for child in children {
        if child.data.role == Role::StaticText {
            if let Some(ref name) = child.data.name {
                if !name.is_empty() {
                    return Some(name.clone());
                }
            }
        }
    }
    None
}

/// Notable positive states to display. The defaults for a healthy interactive
/// element (enabled, showing, visible, sensitive, focusable) are omitted as
/// noise; `get_state_info` reports their absence (`disabled`/`hidden`) plus any
/// of these that are set.
const NOTABLE_STATES: [(u32, &str); 14] = [
    (STATE_FOCUSED, "focused"),
    (STATE_SELECTED, "selected"),
    (STATE_SELECTABLE, "selectable"),
    (STATE_CHECKED, "checked"),
    (STATE_CHECKABLE, "checkable"),
    (STATE_PRESSED, "pressed"),
    (STATE_EXPANDED, "expanded"),
    (STATE_COLLAPSED, "collapsed"),
    (STATE_EDITABLE, "editable"),
    (STATE_MODAL, "modal"),
    (STATE_READ_ONLY, "readonly"),
    (STATE_INDETERMINATE, "indeterminate"),
    (STATE_IS_DEFAULT, "default"),
    (STATE_HAS_POPUP, "has-popup"),
];

/// Decode the `StateSet` bitmask (`au` words) into the set of `StateType` values.
fn decode_state_set(words: &[u32]) -> HashSet<u32> {
    let mut set = HashSet::new();
    for (word_idx, &word) in words.iter().enumerate() {
        for bit in 0..32_u32 {
            if word & (1_u32 << bit) != 0 {
                set.insert(u32::try_from(word_idx).unwrap_or(0) * 32 + bit);
            }
        }
    }
    set
}

/// Query the raw `StateSet` bitmask for an accessible. Returns `None` if the
/// D-Bus call fails or the body can't be deserialized.
fn get_state_set(conn: &Connection, destination: &str, path: &str) -> Option<HashSet<u32>> {
    crate::trace!("ENTER get_state_set {path}");
    let reply = conn.call_method(
        Some(destination),
        path,
        Some("org.a11y.atspi.Accessible"),
        "GetState",
        &(),
    );
    let reply = reply.ok()?;
    let words: Vec<u32> = reply.body().deserialize().ok()?;
    Some(decode_state_set(&words))
}

/// Format a decoded `StateSet` as the compact state string for the attributes
/// list. Reports deviations from the defaults (`disabled` if not enabled,
/// `hidden` if not showing) plus notable positive states. A normal enabled,
/// on-screen element with no notable state returns an empty string.
fn format_state(set: &HashSet<u32>) -> String {
    let mut names: Vec<&str> = Vec::new();
    if !set.contains(&STATE_ENABLED) {
        names.push("disabled");
    }
    if !set.contains(&STATE_SHOWING) {
        names.push("hidden");
    }
    for &(state, label) in &NOTABLE_STATES {
        if set.contains(&state) {
            names.push(label);
        }
    }
    names.join(",")
}

// ---------------------------------------------------------------------------
// Ref resolution (for action dispatch)
// ---------------------------------------------------------------------------

/// Resolve a ref to its `(bus, path)` handle, using a retained handle from the
/// last snapshot when available (O(1)), else a full tree re-walk.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree.
pub(super) fn resolve_ref_atspi(
    ref_id: i32,
    app: &AppTarget,
    cached: Option<AtspiRef>,
) -> Result<AtspiRef, ForepawError> {
    if let Some(handle) = cached {
        return Ok(handle);
    }
    resolve_ref_atspi_rewalk(ref_id, app)
}

/// Fallback re-walk when no retained handle is cached (e.g. resolve before any
/// snapshot on this provider). Best-effort: walks from the app's root at the
/// default depth, numbering interactive nodes like `RefAssigner`.
///
/// This cannot reproduce the pruning a caller's `snapshot` used (depth,
/// zero-size, offscreen): `resolve_ref_*` takes only the ref and app, and
/// `forepaw` is a library, so the caller may have snapshotted with any
/// `SnapshotOptions`. The cached path is exact (same walk that built the tree);
/// when it's absent, treat re-walk resolution as approximate.
fn resolve_ref_atspi_rewalk(ref_id: i32, app: &AppTarget) -> Result<AtspiRef, ForepawError> {
    let conn = connect_atspi_bus()?;
    let app_bus = find_app_bus(&conn, app)?;
    // Match the default snapshot's pruning: offscreen elements are leaf-pruned.
    // Qt exposes closed menus fully in the AT-SPI2 tree, so descending into
    // offscreen subtrees would number their items and desync the rewalk from
    // the snapshot. Use the app's main window bounds, same as `snapshot()` does
    // when no explicit override is given.
    let window_bounds = find_main_window_bounds(&conn, &app_bus);
    let mut handles: HashMap<i32, AtspiRef> = HashMap::new();
    let mut counter: i32 = 1;
    collect_atspi_refs(
        &conn,
        &app_bus,
        "/org/a11y/atspi/accessible/root",
        0,
        SnapshotOptions::default().max_depth,
        window_bounds,
        &mut counter,
        &mut handles,
    );
    handles
        .remove(&ref_id)
        .ok_or_else(|| ForepawError::StaleRef(ElementRef::new(ref_id)))
}

/// Walk the AT-SPI2 tree, collecting `(bus, path)` handles for interactive
/// elements in depth-first order. Must mirror the order used by `RefAssigner`
/// on `build_tree`'s output, including the offscreen leaf-prune and the
/// exclusion of interactive elements with no bounds (Qt's hidden duplicates).
#[expect(clippy::too_many_arguments, reason = "recursive tree-walk helper")]
fn collect_atspi_refs(
    conn: &Connection,
    app_bus: &str,
    path: &str,
    depth: usize,
    max_depth: usize,
    window_bounds: Option<Rect>,
    counter: &mut i32,
    handles: &mut HashMap<i32, AtspiRef>,
) {
    if depth >= max_depth {
        return;
    }
    let role = atspi_role_to_role(get_role(conn, app_bus, path));
    // Match `build_tree`'s menu-bar skip (the common `-i` case). Without this
    // the rewalk numbers menu items the snapshot pruned, desyncing every ref.
    // Like the offscreen leaf-prune below, this matches the common case; the
    // proper fix is threading SnapshotOptions through resolve_ref_*.
    if role == Role::MenuBar {
        return;
    }
    let bounds = get_bounds(conn, app_bus, path);
    // Match `build_tree`'s parent-level filter: interactive elements with no
    // bounds (Qt's hidden duplicate toolbars report 0×0) are excluded entirely.
    if role.is_interactive() && bounds.is_none() {
        return;
    }
    // Match `check_pruned`'s offscreen leaf-prune: an offscreen element keeps
    // its ref (if interactive) but its subtree isn't expanded. Without this the
    // rewalk descends into offscreen closed menus and numbers their items,
    // desyncing from the default snapshot.
    let offscreen = depth > 1
        && match (&window_bounds, &bounds) {
            (Some(wb), Some(b)) => is_offscreen(b, wb),
            _ => false,
        };
    if role.is_interactive() {
        handles.insert(
            *counter,
            AtspiRef {
                bus: app_bus.to_owned(),
                path: path.to_owned(),
            },
        );
        *counter += 1;
    }
    if offscreen {
        return;
    }
    let Ok(children) = get_children(conn, app_bus, path) else {
        return;
    };
    for (child_bus, child_path) in &children {
        // Mirror build_tree's bus resolution: Qt apps use per-app bus names.
        let bus = if child_bus.starts_with(':') && child_bus != app_bus {
            child_bus.clone()
        } else {
            app_bus.to_owned()
        };
        collect_atspi_refs(
            conn,
            &bus,
            child_path,
            depth + 1,
            max_depth,
            window_bounds,
            counter,
            handles,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_mapping_covers_interactive_types() {
        assert_eq!(atspi_role_to_role(43), Role::Button);
        assert_eq!(atspi_role_to_role(79), Role::TextField);
        assert_eq!(atspi_role_to_role(44), Role::RadioButton);
        assert_eq!(atspi_role_to_role(45), Role::MenuItemRadio);
        assert_eq!(atspi_role_to_role(11), Role::ComboBox);
        assert_eq!(atspi_role_to_role(51), Role::Slider);
        assert_eq!(atspi_role_to_role(91), Role::TreeItem);
        assert_eq!(atspi_role_to_role(88), Role::Link);
    }

    #[test]
    fn role_mapping_covers_structural_types() {
        assert_eq!(atspi_role_to_role(116), Role::StaticText);
        assert_eq!(atspi_role_to_role(34), Role::MenuBar);
        assert_eq!(atspi_role_to_role(39), Role::Group);
        assert_eq!(atspi_role_to_role(55), Role::Table);
        assert_eq!(atspi_role_to_role(23), Role::Frame);
        assert_eq!(atspi_role_to_role(69), Role::Window);
    }

    #[test]
    fn unknown_role_maps_to_unknown() {
        assert_eq!(atspi_role_to_role(0), Role::Unknown);
        assert_eq!(atspi_role_to_role(999), Role::Unknown);
    }

    #[test]
    fn interactive_roles_mapped_correctly() {
        assert!(atspi_role_to_role(43).is_interactive()); // Button
        assert!(atspi_role_to_role(79).is_interactive()); // TextField
        assert!(atspi_role_to_role(44).is_interactive()); // RadioButton
        assert!(atspi_role_to_role(11).is_interactive()); // ComboBox
        assert!(atspi_role_to_role(91).is_interactive()); // TreeItem
        assert!(atspi_role_to_role(88).is_interactive()); // Link
    }

    #[test]
    fn structural_roles_not_interactive() {
        assert!(!atspi_role_to_role(23).is_interactive()); // Frame
        assert!(!atspi_role_to_role(39).is_interactive()); // Group
        assert!(!atspi_role_to_role(116).is_interactive()); // StaticText
        assert!(!atspi_role_to_role(27).is_interactive()); // Image
    }
}
