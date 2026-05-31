//! AT-SPI2 tree snapshot: walk the accessibility tree and build an `ElementTree`.
//!
//! Connects to the AT-SPI2 bus, finds the target application, and walks its
//! accessible tree using `GetChildren` + property reads via D-Bus.
//! Maps AT-SPI2 role constants to `Role` variants so the core
//! ref assigner and tree renderer work unchanged across platforms.

use zbus::blocking::Connection;

use crate::core::element_tree::{ElementData, ElementNode, ElementTree, SnapshotTiming};
use crate::core::errors::ForepawError;
use crate::core::ref_assigner::RefAssigner;
use crate::core::role::Role;
use crate::core::types::Rect;
use crate::platform::{AppTarget, SnapshotOptions, WindowTarget};

use super::app::{
    connect_atspi_bus, find_app_bus, find_child_window, get_bounds, get_children, get_property,
    get_role, get_value,
};
use super::role::atspi_role_to_role;

// AT-SPI2 role mapping is generated from res/atspi-constants.h.
// See src/platform/linux/role.rs.

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
/// If `window` is specified, the tree walk is scoped to that specific window
/// (child frame/window) instead of the full app tree.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::WindowNotFound`] if the specified window is not found,
/// or [`ForepawError::ActionFailed`] if the AT-SPI2 bus is unreachable.
pub fn snapshot(
    app: &AppTarget,
    window: Option<&WindowTarget>,
    options: &SnapshotOptions,
) -> Result<ElementTree, ForepawError> {
    let conn = connect_atspi_bus()?;

    // Find the target app's bus name.
    let app_bus = find_app_bus(&conn, app)?;

    // If a window target is specified, resolve it to a child frame/window path.
    let (root_path, window_bounds): (String, Option<Rect>) = if let Some(target) = window {
        find_child_window(&conn, &app_bus, target)?
    } else {
        (
            "/org/a11y/atspi/accessible/root".to_owned(),
            options.window_bounds,
        )
    };

    let pruning = TreePruning {
        skip_zero_size: options.skip_zero_size,
        skip_offscreen: options.skip_offscreen,
        window_bounds,
    };

    let walk_start = std::time::Instant::now();
    let root = build_tree(&conn, &app_bus, &root_path, 0, options.max_depth, &pruning);
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
        return ElementNode::new(ElementData::new(Role::Group));
    }

    let role_num = get_role(conn, app_bus, path);
    let role = atspi_role_to_role(role_num);

    // Get properties
    let name = get_property(conn, app_bus, path, "Name");
    let value = get_value(conn, app_bus, path);
    let bounds = get_bounds(conn, app_bus, path);

    // Check pruning conditions (zero-size and offscreen).
    if let Some(pruned) = check_pruned(role, name.as_ref(), bounds.as_ref(), depth, pruning) {
        return pruned;
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
        data: ElementData {
            role,
            name: final_name,
            value,
            r#ref: None,
            bounds,
            // TODO: populate from AT-SPI2 StateSet (ENABLED, FOCUSED, SELECTED) and Description
            enabled: None,
            focused: None,
            selected: None,
            description: None,
            native_role: None,
            identifier: None,
            attributes,
        },
        children,
    }
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
                        .with_name_opt(name.cloned())
                        .with_bounds(*b),
                ));
            }
        }
    }

    // Prune offscreen elements
    if pruning.skip_offscreen && depth > 1 {
        if let (Some(wb), Some(b)) = (&pruning.window_bounds, bounds) {
            let no_horizontal = b.x + b.width <= wb.x || b.x >= wb.x + wb.width;
            let no_vertical = b.y + b.height <= wb.y || b.y >= wb.y + wb.height;
            if no_horizontal || no_vertical {
                return Some(ElementNode::new(
                    ElementData::new(role)
                        .with_name_opt(name.cloned())
                        .with_bounds(*b),
                ));
            }
        }
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

    if role.is_interactive() {
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
