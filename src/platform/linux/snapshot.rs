//! AT-SPI2 tree snapshot: walk the accessibility tree and build an `ElementTree`.
//!
//! Connects to the AT-SPI2 bus, finds the target application, and walks its
//! accessible tree using `GetChildren` + property reads via D-Bus.
//! Maps AT-SPI2 role constants to AX-prefixed role names so the core
//! ref assigner and tree renderer work unchanged across platforms.

use zbus::blocking::Connection;

use crate::core::element_tree::{is_interactive_role, ElementNode, ElementTree, SnapshotTiming};
use crate::core::errors::ForepawError;
use crate::core::ref_assigner::RefAssigner;
use crate::core::types::Rect;
use crate::platform::AppTarget;
use crate::platform::SnapshotOptions;

use super::app::{connect_atspi_bus, find_app_bus, get_bounds, get_children, get_property, get_role, get_value};
use super::atspi_roles::atspi_role_to_role;

// AT-SPI2 role mapping is generated from res/atspi-constants.h.
// See src/platform/linux/atspi_roles.rs.

// ---------------------------------------------------------------------------
// Name resolution helpers
// ---------------------------------------------------------------------------

/// Get the help text property for name resolution.
fn get_help_text(conn: &Connection, destination: &str, path: &str) -> Option<String> {
    get_property(conn, destination, path, "HelpText")
}

// ---------------------------------------------------------------------------
// Tree pruning
// ---------------------------------------------------------------------------

struct TreePruning {
    skip_zero_size: bool,
    skip_offscreen: bool,
    window_bounds: Option<Rect>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Walk the AT-SPI2 tree for the given app and return an `ElementTree`.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// or [`ForepawError::ActionFailed`] if the AT-SPI2 bus is unreachable.
pub fn snapshot(app: &AppTarget, options: &SnapshotOptions) -> Result<ElementTree, ForepawError> {
    let conn = connect_atspi_bus()?;

    // Find the target app's bus name.
    let app_bus = find_app_bus(&conn, app)?;

    // Build pruning config.
    let window_bounds = options.window_bounds;
    let pruning = TreePruning {
        skip_zero_size: options.skip_zero_size,
        skip_offscreen: options.skip_offscreen,
        window_bounds,
    };

    let walk_start = std::time::Instant::now();
    let root = build_tree(
        &conn,
        &app_bus,
        "/org/a11y/atspi/accessible/root",
        0,
        options.max_depth,
        &pruning,
    );
    let walk_ms = walk_start.elapsed().as_secs_f64() * 1000.0;

    let assigner = RefAssigner::new();
    let result = assigner.assign(&root, options.interactive_only);

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

    Ok(ElementTree {
        app: app.display(),
        root: result.root,
        refs: result.refs,
        window_bounds,
        timing,
    })
}

// ---------------------------------------------------------------------------
// Tree walk
// ---------------------------------------------------------------------------

fn build_tree(
    conn: &Connection,
    app_bus: &str,
    path: &str,
    depth: usize,
    max_depth: usize,
    pruning: &TreePruning,
) -> ElementNode {
    if depth >= max_depth {
        return ElementNode::new("AXGroup");
    }

    let role_num = get_role(conn, app_bus, path);
    let role = atspi_role_to_role(role_num);

    // Get properties
    let name = get_property(conn, app_bus, path, "Name");
    let value = get_value(conn, app_bus, path);
    let bounds = get_bounds(conn, app_bus, path);

    // Prune zero-size subtrees
    if pruning.skip_zero_size {
        if let Some(b) = &bounds {
            if b.width == 0.0 && b.height == 0.0 && depth > 1 {
                return ElementNode {
                    role: role.to_owned(),
                    name: name.clone(),
                    value: None,
                    r#ref: None,
                    bounds,
                    attributes: Vec::new(),
                    children: Vec::new(),
                };
            }
        }
    }

    // Prune offscreen elements
    if pruning.skip_offscreen && depth > 1 {
        if let (Some(wb), Some(b)) = (&pruning.window_bounds, &bounds) {
            let no_horizontal = b.x + b.width <= wb.x || b.x >= wb.x + wb.width;
            let no_vertical = b.y + b.height <= wb.y || b.y >= wb.y + wb.height;
            if no_horizontal || no_vertical {
                return ElementNode {
                    role: role.to_owned(),
                    name: name.clone(),
                    value: None,
                    r#ref: None,
                    bounds,
                    attributes: Vec::new(),
                    children: Vec::new(),
                };
            }
        }
    }

    // Build children first (so name resolution can use them)
    let children_paths = get_children(conn, app_bus, path).unwrap_or_default();
    let children: Vec<ElementNode> = children_paths
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
        .collect();

    // Name resolution: Name → Description → HelpText → first text child
    let final_name = if let Some(ref n) = name {
        if n.is_empty() {
            get_property(conn, app_bus, path, "Description")
                .or_else(|| get_help_text(conn, app_bus, path))
                .or_else(|| first_text_child_name(&children))
        } else {
            Some(n.clone())
        }
    } else {
        get_property(conn, app_bus, path, "Description")
            .or_else(|| get_help_text(conn, app_bus, path))
            .or_else(|| first_text_child_name(&children))
    };

    // Collect attributes (state, interfaces, etc.)
    let mut attributes: Vec<(String, String)> = Vec::new();
    let state = get_state_info(conn, app_bus, path);
    if !state.is_empty() {
        attributes.push(("state".to_owned(), state));
    }

    ElementNode {
        role: role.to_owned(),
        name: final_name,
        value,
        r#ref: None,
        bounds,
        attributes,
        children,
    }
}

/// Get the first child that looks like a text label.
fn first_text_child_name(children: &[ElementNode]) -> Option<String> {
    for child in children {
        if child.role == "AXStaticText" {
            if let Some(ref name) = child.name {
                if !name.is_empty() {
                    return Some(name.clone());
                }
            }
        }
    }
    None
}

/// Get state info as a compact string for the attributes list.
fn get_state_info(conn: &Connection, destination: &str, path: &str) -> String {
    let reply = conn.call_method(
        Some(destination),
        path,
        Some("org.a11y.atspi.Accessible"),
        "GetState",
        &(),
    );
    match reply {
        Ok(r) => {
            let states: Vec<u32> = r.body().deserialize().unwrap_or_default();
            // Convert state bits to readable names
            let state_names: Vec<&str> = states
                .iter()
                .filter_map(|&bit| match bit {
                    0 => Some("sticky"),
                    1 => Some("visible"),
                    2 => Some("manages-descendants"),
                    3 => Some("critical-focus"),
                    4 => Some("focused"),
                    5 => Some("selectable"),
                    6 => Some("selected"),
                    7 => Some("enabled"),
                    8 => Some("required"),
                    9 => Some("tristate"),
                    10 => Some("editable"),
                    11 => Some("expandable"),
                    12 => Some("expanded"),
                    13 => Some("modal"),
                    14 => Some("checkable"),
                    _ => None,
                })
                .collect();
            state_names.join(",")
        }
        Err(_) => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Ref resolution (for action dispatch)
// ---------------------------------------------------------------------------

/// Re-walk the AT-SPI2 tree to count interactive elements up to a given ref ID.
/// Returns true if the ref exists in the tree.
#[must_use]
pub fn ref_exists(app_bus: &str, ref_id: i32, conn: &Connection) -> bool {
    let mut counter: i32 = 1;
    ref_exists_recursive(
        conn,
        app_bus,
        "/org/a11y/atspi/accessible/root",
        0,
        15,
        ref_id,
        &mut counter,
    )
}

fn ref_exists_recursive(
    conn: &Connection,
    app_bus: &str,
    path: &str,
    depth: usize,
    max_depth: usize,
    target: i32,
    counter: &mut i32,
) -> bool {
    if depth >= max_depth {
        return false;
    }

    let role_num = get_role(conn, app_bus, path);
    let role = atspi_role_to_role(role_num);

    if is_interactive_role(role) {
        if *counter == target {
            return true;
        }
        *counter += 1;
    }

    let Ok(children) = get_children(conn, app_bus, path) else {
        return false;
    };

    for (_bus, child_path) in &children {
        if ref_exists_recursive(
            conn,
            app_bus,
            child_path.as_str(),
            depth + 1,
            max_depth,
            target,
            counter,
        ) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_mapping_covers_interactive_types() {
        assert_eq!(atspi_role_to_role(43), "AXButton");
        assert_eq!(atspi_role_to_role(79), "AXTextField");
        assert_eq!(atspi_role_to_role(44), "AXCheckBox");
        assert_eq!(atspi_role_to_role(45), "AXRadioButton");
        assert_eq!(atspi_role_to_role(11), "AXComboBox");
        assert_eq!(atspi_role_to_role(51), "AXSlider");
        assert_eq!(atspi_role_to_role(91), "AXTreeItem");
        assert_eq!(atspi_role_to_role(88), "AXLink");
    }

    #[test]
    fn role_mapping_covers_structural_types() {
        assert_eq!(atspi_role_to_role(116), "AXStaticText");
        assert_eq!(atspi_role_to_role(34), "AXMenuBar");
        assert_eq!(atspi_role_to_role(39), "AXGroup");
        assert_eq!(atspi_role_to_role(55), "AXTable");
        assert_eq!(atspi_role_to_role(23), "AXFrame");
        assert_eq!(atspi_role_to_role(69), "AXWindow");
    }

    #[test]
    fn unknown_role_maps_to_ax_unknown() {
        assert_eq!(atspi_role_to_role(0), "AXUnknown");
        assert_eq!(atspi_role_to_role(999), "AXUnknown");
    }

    #[test]
    fn interactive_roles_mapped_correctly() {
        use crate::core::element_tree::is_interactive_role;

        assert!(is_interactive_role(atspi_role_to_role(43))); // Button
        assert!(is_interactive_role(atspi_role_to_role(79))); // TextField
        assert!(is_interactive_role(atspi_role_to_role(44))); // CheckBox
        assert!(is_interactive_role(atspi_role_to_role(11))); // ComboBox
        assert!(is_interactive_role(atspi_role_to_role(91))); // TreeItem
        assert!(is_interactive_role(atspi_role_to_role(88))); // Link
    }

    #[test]
    fn structural_roles_not_interactive() {
        use crate::core::element_tree::is_interactive_role;

        assert!(!is_interactive_role(atspi_role_to_role(23))); // Frame
        assert!(!is_interactive_role(atspi_role_to_role(39))); // Group
        assert!(!is_interactive_role(atspi_role_to_role(116))); // StaticText
        assert!(!is_interactive_role(atspi_role_to_role(27))); // Image
    }
}
