//! AT-SPI2 element actions: invoke an element's action (click), set its text
//! value (type), and resolve refs to positions/bounds.
//!
//! These go through the AT-SPI2 D-Bus interfaces (`Action`, `EditableText`,
//! `Component`), not input injection — so they bypass the compositor entirely
//! and work from any process with D-Bus access (including SSH sessions), with
//! no permissions. They mirror macOS `AXPress`/`AXSetValue` and Windows
//! `InvokePattern`/`ValuePattern` as the preferred element-action path.
//!
//! `DoAction` takes only an index (no button, no click count), so it covers a
//! plain left-click; right-click and double-click are not expressible here.

use zbus::blocking::Connection;

use crate::core::element_tree::ElementRef;
use crate::core::errors::ForepawError;
use crate::core::key_combo::{ClickOptions, MouseButton};
use crate::core::types::{Point, Rect};
use crate::platform::{ActionResult, AppTarget};

use super::app::{
    connect_atspi_bus, find_app_bus, get_bounds, get_children, get_property, get_role, ROLE_FRAME,
    ROLE_WINDOW,
};
use super::compositor;
use super::input;
use super::role::atspi_role_to_role;
use super::snapshot::{resolve_ref_atspi, AtspiRef};

// ---------------------------------------------------------------------------
// Ref → position / bounds
// ---------------------------------------------------------------------------

/// Activate an app by requesting keyboard focus on its main window via
/// AT-SPI2 `Component.GrabFocus`. Raw uinput events go to whichever window has
/// keyboard focus, so the target must be focused before injecting. This is
/// compositor-agnostic (goes through the a11y bus); whether it also *raises*
/// the window is compositor-dependent.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// or [`ForepawError::ActionFailed`] if the AT-SPI2 bus is unreachable.
pub(super) fn activate(app: &AppTarget) -> Result<(), ForepawError> {
    let conn = connect_atspi_bus()?;
    let app_bus = find_app_bus(&conn, app)?;
    let children = get_children(&conn, &app_bus, "/org/a11y/atspi/accessible/root")?;
    for (_child_bus, path) in &children {
        let role = get_role(&conn, &app_bus, path.as_str());
        if role == ROLE_FRAME || role == ROLE_WINDOW {
            // Best-effort: GrabFocus reports unreliable booleans (see
            // `grab_focus`), but completing the call is what focuses the window.
            let _ = grab_focus(&conn, &app_bus, path.as_str());
            return Ok(());
        }
    }
    Ok(())
}

/// Format the resolved element's role + name for action result messages. Refs
/// are positional and can resolve to a different element if the tree changes
/// between snapshot and action; including the identity lets the caller confirm
/// the ref landed on the expected element.
fn element_identity(conn: &Connection, atspi_ref: &AtspiRef) -> String {
    let role = atspi_role_to_role(get_role(conn, &atspi_ref.bus, &atspi_ref.path));
    let name = get_property(conn, &atspi_ref.bus, &atspi_ref.path, "Name")
        .or_else(|| get_property(conn, &atspi_ref.bus, &atspi_ref.path, "Description"))
        .unwrap_or_else(|| "<unnamed>".to_owned());
    format!("[{role}] {name:?}")
}

/// Resolve a ref to its center point in screen coordinates.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the element has no bounds.
pub(super) fn resolve_ref_position(
    reference: ElementRef,
    app: &AppTarget,
    cached: Option<AtspiRef>,
) -> Result<Point, ForepawError> {
    let bounds = resolve_ref_bounds(reference, app, cached)?;
    Ok(bounds.center())
}

/// Resolve a ref to its bounding rect in screen coordinates.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the element has no bounds.
pub(super) fn resolve_ref_bounds(
    reference: ElementRef,
    app: &AppTarget,
    cached: Option<AtspiRef>,
) -> Result<Rect, ForepawError> {
    let conn = connect_atspi_bus()?;
    let atspi_ref = resolve_ref_atspi(reference.id, app, cached)?;
    get_bounds(&conn, &atspi_ref.bus, &atspi_ref.path)
        .ok_or_else(|| ForepawError::ActionFailed(format!("{reference} has no bounds")))
}

// ---------------------------------------------------------------------------
// Click via AT-SPI2 Action.DoAction
// ---------------------------------------------------------------------------

/// Click an element identified by ref, via its AT-SPI2 action.
///
/// `DoAction` conveys no button or click count, so only a plain left-click is
/// supported on this path; right-click and double-click return an error.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the element exposes no action or
/// `DoAction` reports failure.
pub(super) fn click_ref(
    reference: ElementRef,
    app: &AppTarget,
    options: &ClickOptions,
    cached: Option<AtspiRef>,
) -> Result<ActionResult, ForepawError> {
    activate(app)?;
    if options.button == MouseButton::Right || options.click_count > 1 {
        return Err(ForepawError::ActionFailed(format!(
            "{reference}: right-click/double-click cannot be expressed as an AT-SPI2 \
             DoAction (it takes only an index)"
        )));
    }

    let conn = connect_atspi_bus()?;
    let atspi_ref = resolve_ref_atspi(reference.id, app, cached)?;
    let identity = element_identity(&conn, &atspi_ref);
    let index = best_action_index(&conn, &atspi_ref)?;
    if do_action(&conn, &atspi_ref.bus, &atspi_ref.path, index) {
        let action = get_action_name(&conn, &atspi_ref.bus, &atspi_ref.path, index)
            .unwrap_or_else(|| "action".to_owned());
        Ok(ActionResult::ok_msg(format!(
            "invoked {reference} {identity} via AT-SPI2 {action}"
        )))
    } else {
        Err(ForepawError::ActionFailed(format!(
            "DoAction D-Bus call failed on {reference} {identity}"
        )))
    }
}

/// Pick the action index to invoke. Most elements expose a single action; for
/// multi-action elements, prefer click-like actions (press/click/activate),
/// then jump (links), then toggle/switch, falling back to index 0.
fn best_action_index(conn: &Connection, atspi_ref: &AtspiRef) -> Result<i32, ForepawError> {
    let n = get_n_actions(conn, &atspi_ref.bus, &atspi_ref.path);
    if n <= 0 {
        return Err(ForepawError::ActionFailed(format!(
            "element {} exposes no AT-SPI2 action",
            atspi_ref.path
        )));
    }
    if n == 1 {
        return Ok(0);
    }
    let mut best = 0_i32;
    let mut best_rank = 0_i32;
    for i in 0..n {
        let rank = match get_action_name(conn, &atspi_ref.bus, &atspi_ref.path, i)
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("press" | "click" | "activate") => 3,
            Some("jump") => 2,
            Some("toggle" | "switch") => 1,
            _ => 0,
        };
        if rank > best_rank {
            best_rank = rank;
            best = i;
        }
    }
    Ok(best)
}

// ---------------------------------------------------------------------------
// Type via AT-SPI2 EditableText.SetTextContents
// ---------------------------------------------------------------------------

/// Set the text value of an element identified by ref, via `SetTextContents`.
/// Replaces the field's entire content (same semantics as macOS `AXSetValue`
/// and Windows `ValuePattern.SetValue`). Focuses the element first (best-effort)
/// so it is the active input target.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the element has no `EditableText`
/// interface or `SetTextContents` reports failure.
pub(super) fn type_ref(
    reference: ElementRef,
    text: &str,
    app: &AppTarget,
    cached: Option<AtspiRef>,
) -> Result<ActionResult, ForepawError> {
    activate(app)?;
    let conn = connect_atspi_bus()?;
    let atspi_ref = resolve_ref_atspi(reference.id, app, cached)?;
    let identity = element_identity(&conn, &atspi_ref);
    let _focused = grab_focus(&conn, &atspi_ref.bus, &atspi_ref.path);
    if set_text_contents(&conn, &atspi_ref.bus, &atspi_ref.path, text) {
        Ok(ActionResult::ok_msg(format!(
            "set text on {reference} {identity} via AT-SPI2 EditableText"
        )))
    } else {
        Err(ForepawError::ActionFailed(format!(
            "SetTextContents on {reference} returned false \
             (no EditableText interface?)"
        )))
    }
}

// ---------------------------------------------------------------------------
// AT-SPI2 D-Bus calls
// ---------------------------------------------------------------------------

/// `org.a11y.atspi.Action.GetNActions` → number of actions the element exposes.
///
/// The AT-SPI2 Action methods box their result in a D-Bus variant (`v`); we
/// deserialize as a `Value` and unwrap one layer (mirroring how `app.rs`
/// handles `Properties.Get`).
fn get_n_actions(conn: &Connection, bus: &str, path: &str) -> i32 {
    let Ok(reply) = conn.call_method(
        Some(bus),
        path,
        Some("org.a11y.atspi.Action"),
        "GetNActions",
        &(),
    ) else {
        return 0;
    };
    let body = reply.body();
    let Ok(value) = body.deserialize::<zbus::zvariant::Value>() else {
        return 0;
    };
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "external zvariant::Value enum; only I32 is meaningful for an action count"
    )]
    match unwrap_variant(value) {
        zbus::zvariant::Value::I32(n) => n,
        _ => 0,
    }
}

/// `org.a11y.atspi.Action.GetName(index)` → localized action name.
fn get_action_name(conn: &Connection, bus: &str, path: &str, index: i32) -> Option<String> {
    let reply = conn.call_method(
        Some(bus),
        path,
        Some("org.a11y.atspi.Action"),
        "GetName",
        &(index),
    );
    let reply = reply.ok()?;
    let body = reply.body();
    let value = body.deserialize::<zbus::zvariant::Value>().ok()?;
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "external zvariant::Value enum; only Str is meaningful for an action name"
    )]
    match unwrap_variant(value) {
        zbus::zvariant::Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// `org.a11y.atspi.Action.DoAction(index)`, fired without waiting for a reply.
///
/// Qt's `DoAction` on a button that opens a modal dialog (e.g. Save As) blocks
/// until the dialog closes; `AXPress`/`Invoke` on macOS/Windows return
/// immediately. We match the latter by sending the call with `NoReplyExpected` —
/// the action still fires (Qt processes the method call), we just don't block
/// on its reply. AT-SPI2's `DoAction` boolean return is unreliable anyway
/// (reports `false` even when the action fires), so nothing is lost by not
/// reading it. Returns whether the message was sent.
fn do_action(conn: &Connection, bus: &str, path: &str, index: i32) -> bool {
    let msg = zbus::Message::method_call(path, "DoAction")
        .and_then(|b| b.destination(bus))
        .and_then(|b| b.interface("org.a11y.atspi.Action"))
        .and_then(|b| b.with_flags(zbus::message::Flags::NoReplyExpected))
        .and_then(|b| b.build(&(index)));
    match msg {
        Ok(m) => conn.send(&m).is_ok(),
        Err(_) => false,
    }
}

/// Unwrap one D-Bus variant layer (`Value::Value`), if present. AT-SPI2 methods
/// may box their return in a variant (`v`); this unwraps it when present and
/// passes bare values through unchanged, so callers can match uniformly.
fn unwrap_variant(value: zbus::zvariant::Value) -> zbus::zvariant::Value {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "external zvariant::Value enum; only Value::Value is unwrapped, all others pass through"
    )]
    match value {
        zbus::zvariant::Value::Value(inner) => *inner,
        other => other,
    }
}

/// Read a boolean from an AT-SPI2 method reply, which may return it bare (`b`)
/// or boxed in a variant (`v`). Tries bare first, then unwraps a variant —
/// `zbus`'s `deserialize::<Value>()` only succeeds for variant bodies, so a
/// bare reply must be read directly.
fn reply_bool(reply: &zbus::Message) -> bool {
    let body = reply.body();
    if let Ok(b) = body.deserialize::<bool>() {
        return b;
    }
    let Ok(value) = body.deserialize::<zbus::zvariant::Value>() else {
        return false;
    };
    matches!(unwrap_variant(value), zbus::zvariant::Value::Bool(true))
}

/// `org.a11y.atspi.EditableText.SetTextContents(text)` → success.
fn set_text_contents(conn: &Connection, bus: &str, path: &str, text: &str) -> bool {
    let Ok(reply) = conn.call_method(
        Some(bus),
        path,
        Some("org.a11y.atspi.EditableText"),
        "SetTextContents",
        &(text),
    ) else {
        return false;
    };
    reply_bool(&reply)
}

/// `org.a11y.atspi.Component.GrabFocus()` → success.
fn grab_focus(conn: &Connection, bus: &str, path: &str) -> bool {
    let Ok(reply) = conn.call_method(
        Some(bus),
        path,
        Some("org.a11y.atspi.Component"),
        "GrabFocus",
        &(),
    ) else {
        return false;
    };
    reply_bool(&reply)
}

// ---------------------------------------------------------------------------
// Coordinate actions (click/hover at points, regions, refs)
// ---------------------------------------------------------------------------

/// The app's main window bounds in **screen-absolute physical pixels**: AT-SPI2
/// size (real, but surface-local `[0,0]` origin for app windows) offset by the
/// compositor's global origin for that window. This bridges the Wayland
/// surface-local coordinate trap so coordinate actions can target real screen
/// positions.
///
/// Walks the app's AT-SPI2 frame/window children and returns the first whose
/// caption resolves to a `KWin` window position.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the app has no window whose
/// position the compositor reports (non-KDE compositor, or compositor-owned
/// surfaces like plasmashell panels that aren't tracked as windows).
fn window_frame_screen_bounds(app: &AppTarget) -> Result<Rect, ForepawError> {
    let conn = connect_atspi_bus()?;
    let app_bus = find_app_bus(&conn, app)?;
    let children = get_children(&conn, &app_bus, "/org/a11y/atspi/accessible/root")?;
    for (_bus, path) in &children {
        let role = get_role(&conn, &app_bus, path);
        if role != ROLE_FRAME && role != ROLE_WINDOW {
            continue;
        }
        let title = get_property(&conn, &app_bus, path, "Name").unwrap_or_default();
        let Some(atspi_bounds) = get_bounds(&conn, &app_bus, path) else {
            continue;
        };
        if let Some(origin) = compositor::window_origin_for_caption(&title)? {
            return Ok(Rect::new(
                origin.x,
                origin.y,
                atspi_bounds.width,
                atspi_bounds.height,
            ));
        }
    }
    Err(ForepawError::ActionFailed(format!(
        "{app}: no window with a resolvable compositor position"
    )))
}

/// Validate that a window-relative point lies within `[0,0]..=(w,h)`.
fn validate_point_in_window(point: &Point, bounds: &Rect) -> Result<(), ForepawError> {
    if point.x < 0.0 || point.y < 0.0 || point.x > bounds.width || point.y > bounds.height {
        return Err(ForepawError::ActionFailed(format!(
            "Point ({:.0}, {:.0}) is outside window bounds (0,0)-({:.0},{:.0})",
            point.x, point.y, bounds.width, bounds.height
        )));
    }
    Ok(())
}

/// Click at a window-relative point within `app`'s window.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::ActionFailed`] if the point is outside the window, the
/// compositor position is unavailable, or a uinput write fails.
pub(super) fn click_at_point(
    point: Point,
    app: &AppTarget,
    options: &ClickOptions,
    dev: &input::UinputDevice,
) -> Result<ActionResult, ForepawError> {
    activate(app)?;
    let bounds = window_frame_screen_bounds(app)?;
    validate_point_in_window(&point, &bounds)?;
    let screen = Point::new(bounds.x + point.x, bounds.y + point.y);
    input::perform_click(dev, screen, options.button, options.click_count)?;
    let label = match (options.button, options.click_count) {
        (MouseButton::Right, _) => "right-clicked",
        (MouseButton::Left, n) if n > 1 => "double-clicked",
        (MouseButton::Left, _) => "clicked",
    };
    Ok(ActionResult::ok_msg(format!(
        "{label} at {:.0},{:.0}",
        point.x, point.y
    )))
}

/// Hover at a point. If `app` is given, coordinates are window-relative;
/// otherwise screen-absolute.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::ActionFailed`] if the point is outside the window, the
/// compositor position is unavailable, or a uinput write fails.
pub(super) fn hover_at_point(
    point: Point,
    app: Option<&AppTarget>,
    dev: &input::UinputDevice,
) -> Result<ActionResult, ForepawError> {
    let target = if let Some(app) = app {
        activate(app)?;
        let bounds = window_frame_screen_bounds(app)?;
        validate_point_in_window(&point, &bounds)?;
        Point::new(bounds.x + point.x, bounds.y + point.y)
    } else {
        point
    };
    input::hover_move(dev, target)?;
    Ok(ActionResult::ok_msg(format!(
        "hovered at {:.0},{:.0}",
        point.x, point.y
    )))
}

/// Click the center of a region (window-relative).
///
/// # Errors
///
/// See [`click_at_point`].
pub(super) fn click_region(
    region: Rect,
    app: &AppTarget,
    options: &ClickOptions,
    dev: &input::UinputDevice,
) -> Result<ActionResult, ForepawError> {
    let center = region.center();
    click_at_point(center, app, options, dev)?;
    Ok(ActionResult::ok_msg(format!(
        "clicked region at {:.0},{:.0}",
        center.x, center.y
    )))
}

/// Hover the center of a region (window-relative).
///
/// # Errors
///
/// See [`hover_at_point`].
pub(super) fn hover_region(
    region: Rect,
    app: &AppTarget,
    dev: &input::UinputDevice,
) -> Result<ActionResult, ForepawError> {
    let center = region.center();
    hover_at_point(center, Some(app), dev)?;
    Ok(ActionResult::ok_msg(format!(
        "hovered region at {:.0},{:.0}",
        center.x, center.y
    )))
}

/// Hover over an element identified by ref. The ref's AT-SPI2 bounds are
/// surface-local for app windows (origin `[0,0]`), so the app window's
/// compositor origin is added to land on the real screen position.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the compositor position is unavailable
/// or a uinput write fails.
pub(super) fn hover_ref(
    reference: ElementRef,
    app: &AppTarget,
    cached: Option<AtspiRef>,
    dev: &input::UinputDevice,
) -> Result<ActionResult, ForepawError> {
    activate(app)?;
    let conn = connect_atspi_bus()?;
    let atspi_ref = resolve_ref_atspi(reference.id, app, cached)?;
    let identity = element_identity(&conn, &atspi_ref);
    let center = get_bounds(&conn, &atspi_ref.bus, &atspi_ref.path)
        .ok_or_else(|| ForepawError::ActionFailed(format!("{reference} has no bounds")))?
        .center();
    let frame = window_frame_screen_bounds(app)?;
    let screen = Point::new(frame.x + center.x, frame.y + center.y);
    input::hover_move(dev, screen)?;
    Ok(ActionResult::ok_msg(format!(
        "hovered {reference} {identity}"
    )))
}
