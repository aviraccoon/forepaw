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

use super::app::{connect_atspi_bus, get_bounds};
use super::snapshot::{resolve_ref_atspi, AtspiRef};

// ---------------------------------------------------------------------------
// Ref → position / bounds
// ---------------------------------------------------------------------------

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
    if options.button == MouseButton::Right || options.click_count > 1 {
        return Err(ForepawError::ActionFailed(format!(
            "{reference}: right-click/double-click cannot be expressed as an AT-SPI2 \
             DoAction (it takes only an index)"
        )));
    }

    let conn = connect_atspi_bus()?;
    let atspi_ref = resolve_ref_atspi(reference.id, app, cached)?;
    let index = best_action_index(&conn, &atspi_ref)?;
    if do_action(&conn, &atspi_ref.bus, &atspi_ref.path, index) {
        let name = get_action_name(&conn, &atspi_ref.bus, &atspi_ref.path, index)
            .unwrap_or_else(|| "action".to_owned());
        Ok(ActionResult::ok_msg(format!(
            "invoked {reference} via AT-SPI2 {name}"
        )))
    } else {
        Err(ForepawError::ActionFailed(format!(
            "DoAction D-Bus call failed on {reference}"
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
    let conn = connect_atspi_bus()?;
    let atspi_ref = resolve_ref_atspi(reference.id, app, cached)?;
    let _focused = grab_focus(&conn, &atspi_ref.bus, &atspi_ref.path);
    if set_text_contents(&conn, &atspi_ref.bus, &atspi_ref.path, text) {
        Ok(ActionResult::ok_msg(format!(
            "set text on {reference} via AT-SPI2 EditableText"
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

/// `org.a11y.atspi.Action.DoAction(index)`. Returns whether the D-Bus call
/// completed — **not** the method's boolean return. AT-SPI2's `DoAction` bool
/// is unreliable (it can report `false` even when the action fires), so we
/// discard it and treat a completed call as success, matching pyatspi (which
/// ignores the return value).
fn do_action(conn: &Connection, bus: &str, path: &str, index: i32) -> bool {
    conn.call_method(
        Some(bus),
        path,
        Some("org.a11y.atspi.Action"),
        "DoAction",
        &(index),
    )
    .is_ok()
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
