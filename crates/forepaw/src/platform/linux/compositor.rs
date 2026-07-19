//! `KWin` compositor D-Bus: screen geometry + window global positions.
//!
//! Wayland app windows report surface-local coordinates via AT-SPI2 (origin
//! `[0,0]`), so coordinate actions and snapshot bounds need the compositor's
//! view of where each window actually sits on screen. `KWin` exposes this on
//! the session bus — reachable from any process with session-bus access
//! (including SSH sessions), no Wayland protocol connection required:
//!
//! - `org.kde.KWin.supportInformation` (at `/KWin`) → plain-text screen
//!   geometry (logical) + scale.
//! - `org.kde.krunner1.Match ""` (at `/WindowsRunner`) → window UUIDs +
//!   captions (each window appears twice at different relevance scores;
//!   dedupe by UUID).
//! - `org.kde.KWin.getWindowInfo "{uuid}"` (at `/KWin`) → per-window global
//!   `x`/`y` (logical, fractional doubles), `width`/`height`, `minimized`.
//!
//! Non-KDE compositors are not supported here; the methods return
//! [`ForepawError::ActionFailed`] so callers can fall back.
//!
//! # Coordinate spaces
//!
//! - uinput absolute positioning maps to **physical** pixels.
//! - `supportInformation.Geometry` is **logical**; `Scale` converts to physical.
//! - `getWindowInfo` `x`/`y` are **logical** (fractional; physical int ÷ scale).
//!
//! So uinput absinfo range = logical geometry × scale (physical), and window
//! origins passed to uinput are `getWindowInfo {x,y} × scale`. AT-SPI2
//! `Component.GetExtents` is already physical but `[0,0]`-origin for app
//! windows — offset by the compositor origin here to make it screen-absolute.

use std::collections::{HashMap, HashSet};

use zbus::blocking::Connection;
use zbus::zvariant::{Array, Value};

use crate::core::errors::ForepawError;
use crate::core::types::{Point, Rect};

/// Connect to the session bus (where `org.kde.KWin` registers).
fn connect() -> Result<Connection, ForepawError> {
    Connection::session()
        .map_err(|e| ForepawError::ActionFailed(format!("failed to connect to session bus: {e}")))
}

/// Full virtual-desktop bounds in **physical** pixels, for uinput absinfo.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `KWin` is unreachable or its
/// `supportInformation` lacks the geometry/scale lines.
pub(super) fn screen_geometry() -> Result<Rect, ForepawError> {
    let conn = connect()?;
    let (lw, lh, scale) = parse_display_info(&conn)?;
    Ok(Rect::new(
        0.0,
        0.0,
        f64::from(lw) * scale,
        f64::from(lh) * scale,
    ))
}

/// Find the **physical** screen origin of the `KWin` window whose caption matches
/// `title`. Matching is case-insensitive substring in either direction; the
/// AT-SPI2 window title and `KWin` caption track each other exactly (both reflect
/// the live window title, including dirty markers), so an exact match is the
/// common case and substring is a safety net. Returns `Ok(None)` if no window
/// matches (caller falls back to the surface-local `[0,0]` origin).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `KWin` is unreachable.
pub(super) fn window_origin_for_caption(title: &str) -> Result<Option<Point>, ForepawError> {
    let conn = connect()?;
    let (_, _, scale) = parse_display_info(&conn)?;
    let needle = title.to_lowercase();
    for (uuid, caption) in list_windows(&conn)? {
        let cap = caption.to_lowercase();
        if cap.contains(&needle) || needle.contains(&cap) {
            if let Some((x, y)) = window_logical_origin(&conn, &uuid)? {
                return Ok(Some(Point::new(x * scale, y * scale)));
            }
        }
    }
    Ok(None)
}

/// Bring the `KWin` window whose caption matches `title` to the front via
/// `org.kde.krunner1.Run("0_{uuid}", "")` — the default "Activate" action.
/// KDE-only; returns `Ok(false)` if no window matches so the caller can fall
/// back to AT-SPI2 `GrabFocus` (which focuses but does not raise on Wayland,
/// so mouse-coordinate actions would click through to whatever's on top).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `KWin` is unreachable or `Run`
/// fails to deserialize its (unit) reply.
pub(super) fn activate_window_for_caption(title: &str) -> Result<bool, ForepawError> {
    let conn = connect()?;
    let needle = title.to_lowercase();
    for (uuid, caption) in list_windows(&conn)? {
        let cap = caption.to_lowercase();
        if cap.contains(&needle) || needle.contains(&cap) {
            let match_id = format!("0_{{{uuid}}}");
            let _: () = conn
                .call_method(
                    Some("org.kde.KWin"),
                    "/WindowsRunner",
                    Some("org.kde.krunner1"),
                    "Run",
                    &(&match_id, ""),
                )
                .map_err(|e| ForepawError::ActionFailed(format!("WindowsRunner.Run: {e}")))?
                .body()
                .deserialize()
                .map_err(|e| ForepawError::ActionFailed(format!("Run body: {e}")))?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Parse `supportInformation` → `(logical_width, logical_height, scale)`.
fn parse_display_info(conn: &Connection) -> Result<(i32, i32, f64), ForepawError> {
    let reply = conn
        .call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "supportInformation",
            &(),
        )
        .map_err(|e| ForepawError::ActionFailed(format!("supportInformation: {e}")))?;
    let info: String = reply
        .body()
        .deserialize()
        .map_err(|e| ForepawError::ActionFailed(format!("supportInformation body: {e}")))?;
    let (w, h) = parse_geometry_line(&info)?;
    let scale = parse_scale_line(&info)?;
    Ok((w, h, scale))
}

/// Enumerate `KWin` windows via `Match ""` → `(uuid, caption)`, deduped (Match
/// returns each window at two relevance scores).
fn list_windows(conn: &Connection) -> Result<Vec<(String, String)>, ForepawError> {
    let reply = conn
        .call_method(
            Some("org.kde.KWin"),
            "/WindowsRunner",
            Some("org.kde.krunner1"),
            "Match",
            &(""),
        )
        .map_err(|e| ForepawError::ActionFailed(format!("WindowsRunner.Match: {e}")))?;
    let body = reply.body();
    // Signature a(sssida{sv}): array of (matchId, caption, icon, relevance,
    // score, props). Walk as Values to skip deserializing the dict tail.
    let arr: Array = body
        .deserialize()
        .map_err(|e| ForepawError::ActionFailed(format!("Match body: {e}")))?;
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for item in arr.iter() {
        let Value::Structure(s) = item else {
            continue;
        };
        let fields = s.fields();
        let (Some(match_id), Some(caption)) = (fields.first(), fields.get(1)) else {
            continue;
        };
        let (Value::Str(match_id), Value::Str(caption)) = (match_id, caption) else {
            continue;
        };
        // matchId is `0_{uuid}`; strip the prefix/suffix to get the bare uuid.
        let Some(uuid) = match_id
            .strip_prefix("0_{")
            .and_then(|r| r.strip_suffix('}'))
        else {
            continue;
        };
        let uuid = uuid.to_owned();
        if seen.insert(uuid.clone()) {
            out.push((uuid, caption.to_string()));
        }
    }
    Ok(out)
}

/// Read a window's **logical** `(x, y)` via `getWindowInfo`. `Ok(None)` if the
/// window is gone, minimized, or reports no position.
fn window_logical_origin(
    conn: &Connection,
    uuid: &str,
) -> Result<Option<(f64, f64)>, ForepawError> {
    let arg = format!("{{{uuid}}}");
    let reply = conn
        .call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "getWindowInfo",
            &(&arg),
        )
        .map_err(|e| ForepawError::ActionFailed(format!("getWindowInfo: {e}")))?;
    let body = reply.body();
    let props: HashMap<String, Value<'_>> = body
        .deserialize()
        .map_err(|e| ForepawError::ActionFailed(format!("getWindowInfo body: {e}")))?;
    if matches!(props.get("minimized"), Some(Value::Bool(true))) {
        return Ok(None);
    }
    let x = prop_f64(&props, "x");
    let y = prop_f64(&props, "y");
    match (x, y) {
        (Some(x), Some(y)) => Ok(Some((x, y))),
        _ => Ok(None),
    }
}

/// Read a `getWindowInfo` dict entry as `f64` (x/y/width/height are doubles).
fn prop_f64(props: &HashMap<String, Value<'_>>, key: &str) -> Option<f64> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "external zvariant::Value enum; only F64 is meaningful for window geometry"
    )]
    match props.get(key)? {
        Value::F64(n) => Some(*n),
        _ => None,
    }
}

/// Parse a `Geometry: X,Y,WxH` line from `supportInformation`.
fn parse_geometry_line(info: &str) -> Result<(i32, i32), ForepawError> {
    let line = info
        .lines()
        .find(|l| l.contains("Geometry:"))
        .ok_or_else(|| {
            ForepawError::ActionFailed("supportInformation has no Geometry line".into())
        })?;
    let rest = line
        .split("Geometry:")
        .nth(1)
        .ok_or_else(|| ForepawError::ActionFailed(format!("Geometry line malformed: {line}")))?
        .trim();
    // "0,0,1092x667"
    let wh = rest
        .split(',')
        .nth(2)
        .ok_or_else(|| ForepawError::ActionFailed(format!("Geometry line malformed: {line}")))?;
    let mut parts = wh.split('x');
    let w: i32 = parts
        .next()
        .unwrap_or("")
        .trim()
        .parse()
        .map_err(|_| ForepawError::ActionFailed(format!("Geometry width malformed: {line}")))?;
    let h: i32 =
        parts.next().unwrap_or("").trim().parse().map_err(|_| {
            ForepawError::ActionFailed(format!("Geometry height malformed: {line}"))
        })?;
    Ok((w, h))
}

/// Parse a `Scale: F` line from `supportInformation`.
fn parse_scale_line(info: &str) -> Result<f64, ForepawError> {
    let line = info
        .lines()
        .find(|l| l.contains("Scale:"))
        .ok_or_else(|| ForepawError::ActionFailed("supportInformation has no Scale line".into()))?;
    let rest = line
        .split("Scale:")
        .nth(1)
        .ok_or_else(|| ForepawError::ActionFailed(format!("Scale line malformed: {line}")))?
        .trim();
    rest.parse::<f64>()
        .map_err(|_| ForepawError::ActionFailed(format!("Scale malformed: {line}")))
}
