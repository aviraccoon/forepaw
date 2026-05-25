//! App and window enumeration via AT-SPI2 D-Bus.
//!
//! Connects to the AT-SPI2 accessibility bus (discovered from the session bus),
//! queries the registry's root object for registered applications, and walks
//! each app's children to find windows (`frame` or `window` roles).

use zbus::blocking::Connection;
use zbus::proxy;
use zbus::zvariant::{ObjectPath, Value};

use crate::core::errors::ForepawError;
use crate::core::types::Rect;
use crate::platform::AppTarget;
use crate::platform::{AppInfo, WindowInfo, WindowTarget};

// ---------------------------------------------------------------------------
// D-Bus proxy definitions
//
// The `#[proxy]` macro generates both `XyzProxy` (async) and
// `XyzProxyBlocking` (sync) types. We use the `*Blocking` variants.
// ---------------------------------------------------------------------------

/// Proxy for `org.a11y.Bus` on the session bus.
#[proxy(
    interface = "org.a11y.Bus",
    default_service = "org.a11y.Bus",
    default_path = "/org/a11y/bus"
)]
trait A11yBus {
    fn get_address(&self) -> zbus::Result<String>;
}

/// Get the PID for a D-Bus bus name via `org.freedesktop.DBus.GetConnectionUnixProcessID`.
pub(super) fn get_pid_for_bus_name(conn: &Connection, bus_name: &str) -> Result<u32, ForepawError> {
    let reply = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "GetConnectionUnixProcessID",
            &(bus_name),
        )
        .map_err(|e| {
            ForepawError::ActionFailed(format!("GetConnectionUnixProcessID for {bus_name}: {e}"))
        })?;
    reply
        .body()
        .deserialize()
        .map_err(|e| ForepawError::ActionFailed(format!("PID deserialization: {e}")))
}

/// Proxy for `org.a11y.atspi.Accessible` on the AT-SPI2 bus.
///
/// Note: `get_children` is not declared here because the proxy macro
/// has a known issue with `Vec<(String, ObjectPath)>` return types
/// (`DynamicDeserialize` not general enough). Instead, we call
/// `GetChildren` via `call_method` directly.
#[proxy(
    interface = "org.a11y.atspi.Accessible",
    default_path = "/org/a11y/atspi/accessible/root"
)]
trait Accessible {
    fn get_role(&self) -> zbus::Result<u32>;
}

/// Proxy for `org.a11y.atspi.Component` on the AT-SPI2 bus.
#[proxy(interface = "org.a11y.atspi.Component")]
trait Component {
    /// Returns extents as `(x, y, width, height)`. `coord_type` 0 = screen.
    fn get_extents(&self, coord_type: u32) -> zbus::Result<(i32, i32, i32, i32)>;
}

// ---------------------------------------------------------------------------
// AT-SPI2 bus connection
// ---------------------------------------------------------------------------

/// Connect to the AT-SPI2 accessibility bus.
pub(crate) fn connect_atspi_bus() -> Result<Connection, ForepawError> {
    let session = Connection::session().map_err(|e| {
        ForepawError::ActionFailed(format!("failed to connect to session bus: {e}"))
    })?;

    let bus_proxy = A11yBusProxyBlocking::new(&session)
        .map_err(|e| ForepawError::ActionFailed(format!("failed to create a11y bus proxy: {e}")))?;

    let address = bus_proxy.get_address().map_err(|e| {
        ForepawError::ActionFailed(format!("failed to get AT-SPI2 bus address: {e}"))
    })?;

    zbus::blocking::connection::Builder::address(address.as_str())
        .map_err(|e| ForepawError::ActionFailed(format!("invalid AT-SPI2 bus address: {e}")))?
        .build()
        .map_err(|e| ForepawError::ActionFailed(format!("failed to connect to AT-SPI2 bus: {e}")))
}

// ---------------------------------------------------------------------------
// Raw D-Bus call for GetChildren (proxy macro can't handle the return type)
// ---------------------------------------------------------------------------

/// Call `org.a11y.atspi.Accessible.GetChildren` via raw D-Bus method.
///
/// Returns `(bus_name, object_path)` pairs. The proxy macro has a known
/// issue with `Vec<(String, ObjectPath)>` deserialization, so we call it
/// directly.
pub(super) fn get_children(
    conn: &Connection,
    destination: &str,
    path: &str,
) -> Result<Vec<(String, ObjectPath<'static>)>, ForepawError> {
    let reply = conn
        .call_method(
            Some(destination),
            path,
            Some("org.a11y.atspi.Accessible"),
            "GetChildren",
            &(),
        )
        .map_err(|e| ForepawError::ActionFailed(format!("GetChildren call failed: {e}")))?;

    let body = reply.body();
    let children: Vec<(String, ObjectPath<'_>)> = body
        .deserialize()
        .map_err(|e| ForepawError::ActionFailed(format!("GetChildren deserialization: {e}")))?;

    // Convert to owned (static-lifetime) values so callers don't borrow the reply.
    let children: Vec<(String, ObjectPath<'static>)> = children
        .into_iter()
        .map(|(s, p)| (s, p.into_owned()))
        .collect();

    Ok(children)
}

/// Get a string property from an AT-SPI2 accessible via D-Bus Properties.Get.
pub(super) fn get_property(
    conn: &Connection,
    destination: &str,
    path: &str,
    property: &str,
) -> Option<String> {
    let reply = conn
        .call_method(
            Some(destination),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.a11y.atspi.Accessible", property),
        )
        .ok()?;

    let body = reply.body();
    let value: Value<'_> = body.deserialize().ok()?;
    match value {
        Value::Str(s) => {
            let s = s.to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    }
}

/// Get the role of an accessible element.
pub(super) fn get_role(conn: &Connection, destination: &str, path: &str) -> u32 {
    let reply = conn.call_method(
        Some(destination),
        path,
        Some("org.a11y.atspi.Accessible"),
        "GetRole",
        &(),
    );
    match reply {
        Ok(r) => r.body().deserialize::<u32>().unwrap_or(0),
        Err(_) => 0,
    }
}

/// Get the bounds (x, y, width, height) of a component.
pub(super) fn get_bounds(conn: &Connection, destination: &str, path: &str) -> Option<Rect> {
    let reply = conn.call_method(
        Some(destination),
        path,
        Some("org.a11y.atspi.Component"),
        "GetExtents",
        &(0u32), // coord_type 0 = screen
    );
    match reply {
        Ok(r) => {
            let (x, y, width, height): (i32, i32, i32, i32) = r.body().deserialize().ok()?;
            let rect = Rect::new(
                f64::from(x),
                f64::from(y),
                f64::from(width),
                f64::from(height),
            );
            if rect.width > 0.0 && rect.height > 0.0 {
                Some(rect)
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Get the value of an accessible element (for text fields, sliders, etc).
pub(super) fn get_value(conn: &Connection, destination: &str, path: &str) -> Option<String> {
    // Try the Value interface first (CurrentValue)
    let reply = conn.call_method(
        Some(destination),
        path,
        Some("org.a11y.atspi.Value"),
        "GetCurrentValue",
        &(),
    );
    if let Ok(r) = reply {
        let body = r.body();
        // Value can be double or string depending on the element
        if let Ok(val) = body.deserialize::<f64>() {
            // Format as integer if whole number
            if val.fract() == 0.0 {
                return Some(format!("{val:.0}"));
            }
            return Some(val.to_string());
        }
        if let Ok(val) = body.deserialize::<String>() {
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

/// Resolve an `AppTarget` (name or PID) to a D-Bus bus name on the AT-SPI2 bus.
pub(super) fn find_app_bus(conn: &Connection, app: &AppTarget) -> Result<String, ForepawError> {
    let children = get_children(
        conn,
        "org.a11y.atspi.Registry",
        "/org/a11y/atspi/accessible/root",
    )?;

    match app {
        AppTarget::Name(query) => {
            for (bus_name, _path) in &children {
                let name = get_property(conn, bus_name, "/org/a11y/atspi/accessible/root", "Name")
                    .unwrap_or_default();
                if name.eq_ignore_ascii_case(query) {
                    return Ok(bus_name.clone());
                }
            }
            Err(ForepawError::AppNotFound(app.display()))
        }
        AppTarget::Pid(pid) => {
            let target_pid = u32::try_from(*pid)
                .map_err(|_| ForepawError::AppNotFound(format!("Invalid PID: {pid}")))?;
            for (bus_name, _path) in &children {
                let bus_pid = get_pid_for_bus_name(conn, bus_name)?;
                if bus_pid == target_pid {
                    return Ok(bus_name.clone());
                }
            }
            Err(ForepawError::AppNotFound(app.display()))
        }
    }
}

// ---------------------------------------------------------------------------
// AT-SPI2 role constants
// ---------------------------------------------------------------------------

/// AT-SPI2 role for top-level window frames (Qt/KDE).
const ROLE_FRAME: u32 = 23;

/// AT-SPI2 role for top-level windows (GTK).
const ROLE_WINDOW: u32 = 69;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Lists all running GUI applications registered with AT-SPI2.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the AT-SPI2 bus is unreachable
/// or D-Bus calls fail.
pub fn list_apps() -> Result<Vec<AppInfo>, ForepawError> {
    let atspi = connect_atspi_bus()?;

    let children = get_children(
        &atspi,
        "org.a11y.atspi.Registry",
        "/org/a11y/atspi/accessible/root",
    )?;

    let mut apps = Vec::new();
    for (bus_name, _path) in &children {
        let name =
            get_accessible_property(&atspi, bus_name, "/org/a11y/atspi/accessible/root", "Name")?;
        let pid = get_pid_for_bus_name(&atspi, bus_name)?;

        #[expect(
            clippy::cast_possible_wrap,
            reason = "PID fits in i32 on all real systems"
        )]
        let pid_i32 = pid as i32;

        apps.push(AppInfo {
            name,
            bundle_id: None,
            pid: pid_i32,
        });
    }

    Ok(apps)
}

/// Lists visible windows, optionally filtered by application name.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the AT-SPI2 bus is unreachable
/// or D-Bus calls fail. Returns [`ForepawError::AppNotFound`] if `app_filter`
/// is provided but no matching application is found.
pub fn list_windows(app: Option<&AppTarget>) -> Result<Vec<WindowInfo>, ForepawError> {
    let atspi = connect_atspi_bus()?;

    let children = get_children(
        &atspi,
        "org.a11y.atspi.Registry",
        "/org/a11y/atspi/accessible/root",
    )?;

    let mut windows = Vec::new();

    for (bus_name, _root_path) in &children {
        let app_name_val =
            get_accessible_property(&atspi, bus_name, "/org/a11y/atspi/accessible/root", "Name")?;

        if let Some(filter) = app {
            match filter {
                AppTarget::Name(name) => {
                    if !app_name_val.eq_ignore_ascii_case(name) {
                        continue;
                    }
                }
                AppTarget::Pid(target_pid) => {
                    let bus_pid = get_pid_for_bus_name(&atspi, bus_name)?;
                    if bus_pid != u32::try_from(*target_pid).unwrap_or(u32::MAX) {
                        continue;
                    }
                }
            }
        }

        let app_children = get_children(&atspi, bus_name, "/org/a11y/atspi/accessible/root")
            .map_err(|e| ForepawError::ActionFailed(format!("children for {bus_name}: {e}")))?;

        for (_child_bus, child_path) in &app_children {
            let child_proxy = AccessibleProxyBlocking::builder(&atspi)
                .destination(bus_name.as_str())
                .map_err(|e| ForepawError::ActionFailed(format!("child proxy dest: {e}")))?
                .path(child_path.as_str())
                .map_err(|e| ForepawError::ActionFailed(format!("child proxy path: {e}")))?
                .build()
                .map_err(|e| ForepawError::ActionFailed(format!("child proxy build: {e}")))?;

            let role = child_proxy
                .get_role()
                .map_err(|e| ForepawError::ActionFailed(format!("child role: {e}")))?;

            if role != ROLE_FRAME && role != ROLE_WINDOW {
                continue;
            }

            let title = get_accessible_property(&atspi, bus_name, child_path.as_str(), "Name")
                .unwrap_or_default();
            let bounds = window_bounds(&atspi, bus_name, child_path);

            windows.push(WindowInfo {
                id: child_path.to_string(),
                title,
                app: app_name_val.clone(),
                bounds,
            });
        }
    }

    Ok(windows)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get a string property from `org.a11y.atspi.Accessible` via raw D-Bus call.
///
/// The proxy macro generates `get_name()` as a method call, but AT-SPI2
/// exposes Name as a D-Bus property. Using `org.freedesktop.DBus.Properties.Get`
/// works reliably across both Qt and GTK apps.
fn get_accessible_property(
    conn: &Connection,
    destination: &str,
    path: &str,
    property: &str,
) -> Result<String, ForepawError> {
    use zbus::zvariant::Value;
    let reply = conn
        .call_method(
            Some(destination),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.a11y.atspi.Accessible", property),
        )
        .map_err(|e| {
            ForepawError::ActionFailed(format!(
                "Properties.Get({property}) on {destination} {path}: {e}"
            ))
        })?;
    let body = reply.body();
    let value: Value<'_> = body.deserialize().map_err(|e| {
        ForepawError::ActionFailed(format!("Properties.Get({property}) deserialization: {e}"))
    })?;
    match value {
        Value::Str(s) => Ok(s.to_string()),
        other => Err(ForepawError::ActionFailed(format!(
            "expected string for {property}, got {:?}",
            other.value_signature()
        ))),
    }
}

/// Find a child frame/window of an app matching the given window target.
///
/// Returns the child's D-Bus object path and bounds.
/// - `WindowTarget::Id(id)`: exact match against the object path
/// - `WindowTarget::Title(title)`: case-insensitive substring match on the window name
///
/// Returns [`ForepawError::WindowNotFound`] if no child matches,
/// or [`ForepawError::AmbiguousWindow`] if multiple title matches.
pub(super) fn find_child_window(
    conn: &Connection,
    bus_name: &str,
    target: &WindowTarget,
) -> Result<(String, Option<Rect>), ForepawError> {
    let children = get_children(conn, bus_name, "/org/a11y/atspi/accessible/root")
        .map_err(|e| ForepawError::ActionFailed(format!("children for {bus_name}: {e}")))?;

    match target {
        WindowTarget::Id(id) => {
            // Match by exact D-Bus object path
            for (_child_bus, child_path) in &children {
                if child_path.as_str() == id {
                    let bounds = window_bounds(conn, bus_name, child_path);
                    return Ok((child_path.to_string(), bounds));
                }
            }
            Err(ForepawError::WindowNotFound(id.to_owned()))
        }
        WindowTarget::Title(pattern) => {
            // Match by title substring (case-insensitive)
            let mut matches: Vec<(String, Option<Rect>)> = Vec::new();
            for (_child_bus, child_path) in &children {
                let proxy = AccessibleProxyBlocking::builder(conn)
                    .destination(bus_name)
                    .map_err(|e| ForepawError::ActionFailed(format!("proxy dest: {e}")))?
                    .path(child_path.as_str())
                    .map_err(|e| ForepawError::ActionFailed(format!("proxy path: {e}")))?
                    .build()
                    .map_err(|e| ForepawError::ActionFailed(format!("proxy: {e}")))?;

                let role = proxy
                    .get_role()
                    .map_err(|e| ForepawError::ActionFailed(format!("role: {e}")))?;

                if role != ROLE_FRAME && role != ROLE_WINDOW {
                    continue;
                }

                let title =
                    get_property(conn, bus_name, child_path.as_str(), "Name").unwrap_or_default();

                if title.to_lowercase().contains(&pattern.to_lowercase()) {
                    let bounds = window_bounds(conn, bus_name, child_path);
                    matches.push((child_path.to_string(), bounds));
                }
            }

            match matches.len() {
                1 => Ok(matches.into_iter().next().expect("len == 1 checked")),
                2.. => {
                    let titles = matches
                        .iter()
                        .map(|(path, _)| format!("  {path}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Err(ForepawError::AmbiguousWindow {
                        query: pattern.to_owned(),
                        matches: titles,
                    })
                }
                0 => Err(ForepawError::WindowNotFound(pattern.to_owned())),
            }
        }
    }
}

/// Get window bounds via the Component interface.
fn window_bounds(atspi: &Connection, bus_name: &str, path: &ObjectPath<'_>) -> Option<Rect> {
    let proxy = ComponentProxyBlocking::builder(atspi)
        .destination(bus_name)
        .ok()?
        .path(path.as_str())
        .ok()?
        .build()
        .ok()?;

    let (x, y, w, h) = proxy.get_extents(0).ok()?;
    Some(Rect::new(
        f64::from(x),
        f64::from(y),
        f64::from(w),
        f64::from(h),
    ))
}
