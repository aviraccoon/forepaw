//! AT-SPI2 hit test: element at point via `Component.GetAccessibleAtPoint`.
//!
//! Queries the AT-SPI2 Component interface on the application root to find the
//! accessible element at a screen coordinate. Walks the parent chain via
//! `Accessible.GetParent`.
//!
//! Coordinate system: screen coordinates (`coord_type`=0 for `GetAccessibleAtPoint`).
//! The caller converts from per-window coords before calling the trait method.
//!
//! Note: GTK3/ATK apps don't implement the Component interface properly (no
//! position data), so `GetAccessibleAtPoint` will return null for those apps.
//! Future: add cached-tree bounds-check fallback for GTK3/ATK.

use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, Value};

use crate::core::errors::ForepawError;
use crate::core::types::Point;
use crate::platform::{AncestorInfo, HitTestResult};

use super::app::connect_atspi_bus;
use super::atspi_roles::atspi_role_to_role;
use super::snapshot::{
    find_app_bus, get_bounds, get_children, get_property, get_role, get_value,
};

/// Performs a hit test at the given screen coordinates.
///
/// Returns the deepest AT-SPI2 accessible element at the point and its ancestor
/// chain. Uses `Component.GetAccessibleAtPoint` on the application root for
/// native hit testing.
///
/// When `app_hint` is `None` (system-wide), iterates all registered apps and
/// queries each one. The first app that reports an element wins (z-order is
/// implicit in app registration order). When scoped to an app name, only queries
/// that app's root component.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if no element is found at the position,
/// the AT-SPI2 bus is unreachable, or `app_hint` is set but the app isn't found.
pub fn element_at_point(
    point: Point,
    app_hint: Option<&str>,
) -> Result<HitTestResult, ForepawError> {
    let conn = connect_atspi_bus()?;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates fit in i32"
    )]
    let x = point.x as i32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates fit in i32"
    )]
    let y = point.y as i32;

    // Get the bus names to query
    let app_buses: Vec<String> = if let Some(app_name) = app_hint {
        vec![find_app_bus(&conn, app_name)?]
    } else {
        // System-wide: iterate all registered apps
        get_children(
            &conn,
            "org.a11y.atspi.Registry",
            "/org/a11y/atspi/accessible/root",
        )
        .map_err(|e| {
            ForepawError::ActionFailed(format!("failed to list AT-SPI2 apps: {e}"))
        })?
        .into_iter()
        .map(|(bus, _path)| bus)
        .collect()
    };

    // Query each app's root for an element at the point
    for app_bus in &app_buses {
        let hit = query_app_at_point(&conn, app_bus, x, y);
        if let Some((hit_bus, hit_path)) = hit {
            return Ok(build_hit_result(&conn, &hit_bus, &hit_path));
        }
    }

    Err(ForepawError::ActionFailed("no element at position".into()))
}

/// Query a single app's root component for an element at (x, y).
fn query_app_at_point(
    conn: &Connection,
    app_bus: &str,
    x: i32,
    y: i32,
) -> Option<(String, ObjectPath<'static>)> {
    let reply = conn
        .call_method(
            Some(app_bus),
            "/org/a11y/atspi/accessible/root",
            Some("org.a11y.atspi.Component"),
            "GetAccessibleAtPoint",
            &(x, y, 0u32), // coord_type = 0 (screen)
        )
        .ok()?;

    let body = reply.body();
    let (result_bus, result_path): (String, ObjectPath<'_>) = body.deserialize().ok()?;

    if result_path.as_str() == "/org/a11y/atspi/null" || result_path.as_str() == "/" {
        return None;
    }

    let bus = if result_bus.is_empty() || result_bus == app_bus {
        app_bus.to_owned()
    } else {
        result_bus
    };

    Some((bus, result_path.into_owned()))
}

/// Build a `HitTestResult` from a found element.
fn build_hit_result(
    conn: &Connection,
    hit_bus: &str,
    hit_path: &str,
) -> HitTestResult {
    let role_num = get_role(conn, hit_bus, hit_path);
    let role = atspi_role_to_role(role_num).to_owned();
    let name = get_property(conn, hit_bus, hit_path, "Name");
    let value = get_value(conn, hit_bus, hit_path);
    let bounds = get_bounds(conn, hit_bus, hit_path);
    #[expect(
        clippy::cast_possible_wrap,
        reason = "PID fits in i32 on all practical systems"
    )]
    let pid = get_pid_for_bus_name(conn, hit_bus).unwrap_or(0) as i32;

    let ancestors = walk_parents(conn, hit_bus, hit_path);

    // TODO: fetch action names via org.a11y.atspi.Action.GetActions
    let actions = Vec::new();

    HitTestResult {
        role,
        name: name.filter(|s| !s.is_empty()),
        value,
        bounds,
        actions,
        ancestors,
        pid,
    }
}

/// Walk the parent chain from the hit element up to the app root.
fn walk_parents(conn: &Connection, hit_bus: &str, hit_path: &str) -> Vec<AncestorInfo> {
    let mut ancestors: Vec<AncestorInfo> = Vec::new();
    let mut current_bus = hit_bus.to_owned();
    let mut current_path = hit_path.to_owned();

    for _ in 0..30 {
        let Some(reply) = conn
            .call_method(
                Some(current_bus.as_str()),
                current_path.as_str(),
                Some("org.a11y.atspi.Accessible"),
                "GetParent",
                &(),
            )
            .ok()
        else {
            break;
        };

        let body = reply.body();

        // AT-SPI2 wraps GetParent's (so) return in a D-Bus variant (v).
        // Value::downcast() transparently strips the variant wrapper.
        let value: Value<'_> = match body.deserialize() {
            Ok(v) => v,
            Err(_) => break,
        };
        let (parent_bus, parent_path): (String, ObjectPath<'_>) = match value.downcast() {
            Ok(v) => v,
            Err(_) => break,
        };

        if parent_path.as_str() == "/" || parent_path.as_str() == "/org/a11y/atspi/null" {
            break;
        }

        let bus = if parent_bus.is_empty() {
            current_bus.clone()
        } else {
            parent_bus
        };
        let path_str = parent_path.into_owned().to_string();

        let parent_role = atspi_role_to_role(get_role(conn, &bus, &path_str)).to_owned();
        let parent_name = get_property(conn, &bus, &path_str, "Name");
        let parent_bounds = get_bounds(conn, &bus, &path_str);

        ancestors.push(AncestorInfo {
            role: parent_role.clone(),
            name: parent_name.filter(|s| !s.is_empty()),
            bounds: parent_bounds,
        });

        if parent_role == "AXApplication" || parent_role == "AXFrame" {
            break;
        }

        current_bus = bus;
        current_path = path_str;
    }

    ancestors.reverse();
    ancestors
}

/// Get the PID for a D-Bus bus name via `GetConnectionUnixProcessID`.
fn get_pid_for_bus_name(conn: &Connection, bus_name: &str) -> Option<u32> {
    let reply = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "GetConnectionUnixProcessID",
            &(bus_name),
        )
        .ok()?;
    reply.body().deserialize::<u32>().ok()
}
