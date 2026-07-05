//! Input simulation via `SendInput` and `SetCursorPos` (keyboard, mouse,
//! and later scroll/drag).
//!
//! Keyboard and mouse-button events go through `SendInput`; cursor
//! positioning uses `SetCursorPos`, which takes physical pixels directly
//! (no 0..65535 normalization) and is correct at any DPI and on any monitor.

use std::mem::size_of;
use std::thread;
use std::time::Duration;

use windows::core::BSTR;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationInvokePattern, IUIAutomationValuePattern,
    UIA_InvokePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos, WHEEL_DELTA};

use crate::core::element_tree::ElementRef;
use crate::core::errors::ForepawError;
use crate::core::key_combo::{ClickOptions, DragOptions, KeyCombo, Modifier, MouseButton};
use crate::core::types::{Point, Rect};
use crate::platform::{ActionResult, AppTarget};

use super::app;
use super::key_code::{modifier_vk, virtual_key_code};
use super::screenshot;
use super::snapshot;

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

/// Type a string character-by-character via `SendInput` Unicode events.
///
/// Each UTF-16 unit of every character is sent as a `KEYEVENTF_UNICODE`
/// key-down/key-up pair (so BMP-outside characters -- emoji, etc. -- send
/// their surrogate pair correctly). An ~8ms inter-character delay is kept,
/// matching the macOS backend: Electron/Chromium apps drop characters when
/// events arrive too fast.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `SendInput` rejects the events.
pub fn type_via_keyboard(text: &str) -> Result<(), ForepawError> {
    let mut buf = [0_u16; 2];
    for ch in text.chars() {
        let utf16 = ch.encode_utf16(&mut buf);
        for &unit in utf16.iter() {
            let down = keyboard_input(0, unit, KEYEVENTF_UNICODE);
            let up = keyboard_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
            send_inputs(&[down, up])?;
        }
        thread::sleep(Duration::from_millis(8));
    }
    Ok(())
}

/// Press a key combo (modifiers + key) via a single batched `SendInput` call:
/// modifier key-downs, main key down, main key up, modifier key-ups (reverse).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the key name is unrecognized or
/// `SendInput` rejects the events.
pub fn press_via_keyboard(combo: &KeyCombo) -> Result<(), ForepawError> {
    let key_vk = virtual_key_code(&combo.key)
        .ok_or_else(|| ForepawError::ActionFailed(format!("unknown key: '{}'", combo.key)))?;
    let mod_vks: Vec<u16> = combo.modifiers.iter().filter_map(modifier_vk).collect();

    let mut inputs: Vec<INPUT> = Vec::with_capacity(mod_vks.len() * 2 + 2);

    // Modifier key-downs
    for &vk in &mod_vks {
        inputs.push(keyboard_input(vk, 0, KEYBD_EVENT_FLAGS(0)));
    }
    // Main key down + up
    inputs.push(keyboard_input(key_vk, 0, KEYBD_EVENT_FLAGS(0)));
    inputs.push(keyboard_input(key_vk, 0, KEYEVENTF_KEYUP));
    // Modifier key-ups in reverse so nested chords release naturally
    for &vk in mod_vks.iter().rev() {
        inputs.push(keyboard_input(vk, 0, KEYEVENTF_KEYUP));
    }

    send_inputs(&inputs)
}

/// Type text via the keyboard into whatever has focus, optionally activating
/// the target app first.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if `app` is given but not running,
/// or [`ForepawError::ActionFailed`] if the platform input API fails.
pub fn keyboard_type(text: &str, app: Option<&AppTarget>) -> Result<ActionResult, ForepawError> {
    if let Some(app) = app {
        app::activate_app(app)?;
    }
    type_via_keyboard(text)?;
    Ok(ActionResult::ok_msg(format!("typed {} chars", text.len())))
}

/// Press a key combo, optionally activating an app first.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the app is specified but not running,
/// or [`ForepawError::ActionFailed`] if the key is unknown or input fails.
pub fn press_key(keys: &KeyCombo, app: Option<&AppTarget>) -> Result<ActionResult, ForepawError> {
    if let Some(app) = app {
        app::activate_app(app)?;
    }
    press_via_keyboard(keys)?;
    Ok(ActionResult::ok())
}

// ---------------------------------------------------------------------------
// Mouse
// ---------------------------------------------------------------------------

/// Move the cursor to a screen-absolute point (physical px) via `SetCursorPos`,
/// no settle. Used for high-frequency moves (drag interpolation) where the
/// caller controls timing. Physical px go in directly (no 0..65535
/// normalization), so this is correct at any DPI and on multi-monitor setups.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `SetCursorPos` rejects the call.
fn set_cursor_pos(point: Point) -> Result<(), ForepawError> {
    let (x, y) = to_pixels(&point);
    // SAFETY: SetCursorPos takes physical pixel coords; any in-range values are safe.
    unsafe { SetCursorPos(x, y) }
        .map_err(|e| ForepawError::ActionFailed(format!("SetCursorPos failed: {e}")))?;
    Ok(())
}

/// Move the cursor and let it settle (50ms). For one-shot positioning
/// (click/hover/scroll target) where hover/enter handlers need time to fire.
fn move_mouse_to(point: Point) -> Result<(), ForepawError> {
    set_cursor_pos(point)?;
    thread::sleep(Duration::from_millis(50));
    Ok(())
}

/// Move the cursor smoothly from its current position to `target`, posting
/// intermediate `SetCursorPos` calls so hover/enter handlers fire along the
/// path.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if a `SetCursorPos` call fails.
fn smooth_move_mouse(target: Point, steps: usize, duration: Duration) -> Result<(), ForepawError> {
    let mut current = POINT::default();
    // SAFETY: GetCursorPos writes to a valid POINT.
    if unsafe { GetCursorPos(&raw mut current) }.is_err() {
        return move_mouse_to(target);
    }
    let start = Point::new(f64::from(current.x), f64::from(current.y));

    #[expect(clippy::cast_possible_truncation, reason = "step count fits in u32")]
    let step_delay = duration / steps as u32;
    for i in 1..=steps {
        #[expect(
            clippy::cast_precision_loss,
            reason = "step index to f64 for interpolation"
        )]
        let t = i as f64 / steps as f64;
        let p = Point::new(
            start.x + (target.x - start.x) * t,
            start.y + (target.y - start.y) * t,
        );
        let (x, y) = to_pixels(&p);
        // SAFETY: SetCursorPos with physical pixel coords.
        unsafe { SetCursorPos(x, y) }
            .map_err(|e| ForepawError::ActionFailed(format!("SetCursorPos failed: {e}")))?;
        thread::sleep(step_delay);
    }
    Ok(())
}

/// Move to `point`, then post button down/up events via `SendInput`.
///
/// Click count > 1 sends repeated down/up pairs; the OS detects multi-clicks
/// by timing (there is no explicit click-state field in `MOUSEINPUT`).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the move or a click event fails.
fn perform_mouse_click(
    point: Point,
    button: MouseButton,
    click_count: u32,
) -> Result<(), ForepawError> {
    move_mouse_to(point)?;
    let (down, up) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
    };
    for i in 1..=click_count {
        send_inputs(&[mouse_input(down)])?;
        send_inputs(&[mouse_input(up)])?;
        if i < click_count {
            thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

/// Click at window-relative coordinates within `app`'s window.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::WindowNotFound`] if the window cannot be resolved,
/// or [`ForepawError::ActionFailed`] if the point falls outside the window.
pub fn click_at_point(
    point: Point,
    app: &AppTarget,
    options: &ClickOptions,
) -> Result<ActionResult, ForepawError> {
    app::activate_app(app)?;
    app::validate_point_in_window(&point, app)?;
    let screen = app::to_screen_point(&point, app)?;
    perform_mouse_click(screen, options.button, options.click_count)?;
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
/// Returns [`ForepawError::ActionFailed`] if `app` is specified but the point
/// falls outside the window, or the mouse move fails.
pub fn hover_at_point(
    point: Point,
    app: Option<&AppTarget>,
    smooth: bool,
) -> Result<ActionResult, ForepawError> {
    let target = if let Some(app) = app {
        app::activate_app(app)?;
        app::validate_point_in_window(&point, app)?;
        app::to_screen_point(&point, app)?
    } else {
        point
    };
    if smooth {
        smooth_move_mouse(target, 20, Duration::from_millis(150))?;
    } else {
        move_mouse_to(target)?;
    }
    Ok(ActionResult::ok_msg(format!(
        "hovered at {:.0},{:.0}",
        point.x, point.y
    )))
}

/// Click the center of a region.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// or [`ForepawError::ActionFailed`] if the region center falls outside the window.
pub fn click_region(
    region: Rect,
    app: &AppTarget,
    _window: Option<&crate::platform::WindowTarget>,
    options: &ClickOptions,
) -> Result<ActionResult, ForepawError> {
    let center = region.center();
    click_at_point(center, app, options)?;
    Ok(ActionResult::ok_msg(format!(
        "clicked region at {:.0},{:.0}",
        center.x, center.y
    )))
}

/// Hover the center of a region.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// or [`ForepawError::ActionFailed`] if the region center falls outside the window.
pub fn hover_region(
    region: Rect,
    app: &AppTarget,
    _window: Option<&crate::platform::WindowTarget>,
    smooth: bool,
) -> Result<ActionResult, ForepawError> {
    let center = region.center();
    hover_at_point(center, Some(app), smooth)?;
    Ok(ActionResult::ok_msg(format!(
        "hovered region at {:.0},{:.0}",
        center.x, center.y
    )))
}

// ---------------------------------------------------------------------------
// Ref-based actions (InvokePattern / ValuePattern with coordinate fallback)
// ---------------------------------------------------------------------------

/// Click a UIA element. Tries `InvokePattern.Invoke()` first, then falls back
/// to a mouse click at the element's center. Right-click and double-click
/// always use the mouse: `Invoke` takes no arguments, so it cannot convey
/// button or click count.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if neither Invoke nor a coordinate
/// click succeeds.
pub fn click_element(
    element: &IUIAutomationElement,
    options: &ClickOptions,
    app: &AppTarget,
) -> Result<ActionResult, ForepawError> {
    let is_right = options.button == MouseButton::Right;
    let is_double = options.click_count > 1;
    let prefer_mouse = is_right || is_double;

    if !prefer_mouse {
        // SAFETY: GetCurrentPatternAs reads a pattern from a valid element.
        let invoke = unsafe {
            element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
        };
        if let Ok(pattern) = invoke {
            // SAFETY: Invoke on a pattern we just retrieved.
            if unsafe { pattern.Invoke() }.is_ok() {
                return Ok(ActionResult::ok_msg("invoked via InvokePattern"));
            }
        }
    }

    // Mouse click at element center.
    if let Some(bounds) = snapshot::get_element_bounds(element) {
        let center = bounds.center();
        perform_mouse_click(center, options.button, options.click_count)?;
        let rel = window_relative(center, app);
        let label = if is_right {
            "right-clicked"
        } else if is_double {
            "double-clicked"
        } else {
            "clicked"
        };
        return Ok(ActionResult::ok_msg(format!(
            "{label} at {:.0},{:.0}",
            rel.x, rel.y
        )));
    }

    Ok(ActionResult::fail(
        "click failed: element has no bounds and Invoke unavailable",
    ))
}

/// Set text on a UIA element. Tries `ValuePattern.SetValue()` first, then
/// falls back to `SetFocus()` + simulated typing.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the value cannot be set and the
/// keyboard fallback also fails.
pub fn set_value_on_element(
    element: &IUIAutomationElement,
    value: &str,
) -> Result<ActionResult, ForepawError> {
    // SAFETY: GetCurrentPatternAs reads a pattern from a valid element.
    let value_pattern =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) };
    if let Ok(pattern) = value_pattern {
        let bstr = BSTR::from_wide(value.encode_utf16().collect::<Vec<u16>>().as_slice());
        // SAFETY: SetValue on a pattern we just retrieved.
        if unsafe { pattern.SetValue(&bstr) }.is_ok() {
            return Ok(ActionResult::ok_msg("set via ValuePattern"));
        }
    }

    // Fallback: focus the element (best-effort), then type. Focus failure is
    // ignored -- if the element can't take focus, `type_via_keyboard` will
    // surface the downstream miss.
    // SAFETY: SetFocus on a valid element.
    let _focus = unsafe { element.SetFocus() };
    type_via_keyboard(value)?;
    Ok(ActionResult::ok_msg("typed via keyboard simulation"))
}

/// Click an element identified by ref. Uses a retained handle from the last
/// snapshot when available (O(1)), else re-walks the tree.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// or [`ForepawError::StaleRef`] if the ref no longer exists.
pub fn click_ref(
    reference: ElementRef,
    app: &AppTarget,
    options: &ClickOptions,
    cached: Option<IUIAutomationElement>,
) -> Result<ActionResult, ForepawError> {
    app::activate_app(app)?;
    let element = snapshot::resolve_ref_element(reference.id, app, cached)?;
    click_element(&element, options, app)
}

/// Hover over an element identified by ref.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the element has no bounds.
pub fn hover_ref(
    reference: ElementRef,
    app: &AppTarget,
    cached: Option<IUIAutomationElement>,
) -> Result<ActionResult, ForepawError> {
    app::activate_app(app)?;
    let element = snapshot::resolve_ref_element(reference.id, app, cached)?;
    let center = snapshot::get_element_bounds(&element)
        .ok_or_else(|| {
            ForepawError::ActionFailed(format!("Cannot determine bounds of {reference}"))
        })?
        .center();
    move_mouse_to(center)?;
    let rel = window_relative(center, app);
    Ok(ActionResult::ok_msg(format!(
        "hovered at {:.0},{:.0}",
        rel.x, rel.y
    )))
}

/// Type text into an element identified by ref (set value or keyboard fallback).
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the value cannot be set.
pub fn type_ref(
    reference: ElementRef,
    text: &str,
    app: &AppTarget,
    cached: Option<IUIAutomationElement>,
) -> Result<ActionResult, ForepawError> {
    app::activate_app(app)?;
    let element = snapshot::resolve_ref_element(reference.id, app, cached)?;
    set_value_on_element(&element, text)
}

/// Translate a screen-absolute (physical px) point to window-relative coords
/// for action result reporting, using the app's best window origin. Falls back
/// to the raw screen point if the window can't be resolved.
fn window_relative(point: Point, app: &AppTarget) -> Point {
    app::find_app_hwnd(app, None)
        .ok()
        .map_or(point, |(_, bounds)| {
            Point::new(point.x - bounds.x, point.y - bounds.y)
        })
}

// ---------------------------------------------------------------------------
// Scroll and drag
// ---------------------------------------------------------------------------

/// Scroll within the app's window. Moves the cursor to the target first (the
/// wheel event goes to the window under the cursor), then posts a
/// `MOUSEEVENTF_WHEEL`/`MOUSEEVENTF_HWHEEL` event. Boundary detection via a
/// screen-strip pixel fingerprint (`BitBlt`).
///
/// Target resolution, in priority order: `at` (window-relative, validated),
/// `reference` (element center via the cache or re-walk), else the window
/// center.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::StaleRef`] if a ref is given but no longer exists,
/// [`ForepawError::ActionFailed`] for an unknown direction or input failure.
pub fn scroll(
    direction: &str,
    amount: u32,
    app: &AppTarget,
    window: Option<&crate::platform::WindowTarget>,
    reference: Option<ElementRef>,
    at: Option<Point>,
    cached: Option<IUIAutomationElement>,
) -> Result<ActionResult, ForepawError> {
    app::activate_app(app)?;
    let (_, bounds) = app::find_app_hwnd(app, window)?;

    let target = if let Some(point) = at {
        app::validate_point_in_window(&point, app)?;
        app::to_screen_point(&point, app)?
    } else if let Some(reference) = reference {
        snapshot::resolve_ref_position(reference.id, app, cached)?
    } else {
        // Default: center of the app's best-matching window.
        bounds.center()
    };

    // `mouseData` holds the wheel delta in WHEEL_DELTA units, packed into the
    // DWORD field as two's complement for negative deltas (Win32 reads it back
    // signed). All u32 arithmetic -- no signed casts.
    let magnitude = amount.saturating_mul(WHEEL_DELTA);
    let (flag, mouse_data) = match direction {
        "up" => (MOUSEEVENTF_WHEEL, magnitude),
        "down" => (MOUSEEVENTF_WHEEL, magnitude.wrapping_neg()),
        "left" => (MOUSEEVENTF_HWHEEL, magnitude.wrapping_neg()),
        "right" => (MOUSEEVENTF_HWHEEL, magnitude),
        _ => {
            return Err(ForepawError::ActionFailed(format!(
                "Unknown direction '{direction}'. Use up, down, left, or right."
            )))
        }
    };

    move_mouse_to(target)?;
    // Fingerprint the content before/after to detect the scroll boundary.
    let before = screenshot::capture_strip_fingerprint(bounds);
    send_inputs(&[mouse_wheel_input(flag, mouse_data)])?;
    thread::sleep(Duration::from_millis(150));
    let at_boundary = matches!((before, screenshot::capture_strip_fingerprint(bounds)), (Some(b), Some(a)) if a == b);

    let rel = window_relative(target, app);
    let note = if at_boundary {
        " (at boundary -- content did not change)"
    } else {
        ""
    };
    Ok(ActionResult::ok_msg(format!(
        "scrolled {direction} {amount} ticks at {:.0},{:.0}{note}",
        rel.x, rel.y
    )))
}

/// Drag along a path of points. Coordinates are window-relative when `app` is
/// given, else screen-absolute.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the path has fewer than 2 points
/// or the platform input API rejects an event.
pub fn drag_path(
    path: &[Point],
    options: &DragOptions,
    app: Option<&AppTarget>,
) -> Result<ActionResult, ForepawError> {
    if path.len() < 2 {
        return Err(ForepawError::ActionFailed(
            "Drag path requires at least 2 points".into(),
        ));
    }

    let screen_path: Vec<Point> = if let Some(app) = app {
        app::activate_app(app)?;
        path.iter()
            .map(|p| app::to_screen_point(p, app))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        path.to_vec()
    };
    let mut screen_path = screen_path;
    if options.close_path && screen_path.len() >= 3 {
        if let Some(first) = screen_path.first().copied() {
            screen_path.push(first);
        }
    }

    perform_mouse_drag(&screen_path, options)?;

    // Report the original (input) coordinates the caller passed.
    let msg = if let [from, to] = path {
        format!(
            "dragged from {:.0},{:.0} to {:.0},{:.0} ({} steps, {:.1}s)",
            from.x, from.y, to.x, to.y, options.steps, options.duration,
        )
    } else {
        format!(
            "dragged through {} points ({} steps/segment, {:.1}s)",
            path.len(),
            options.steps,
            options.duration,
        )
    };
    Ok(ActionResult::ok_msg(msg))
}

/// Drag from one element to another, identified by refs.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// [`ForepawError::StaleRef`] if either ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the drag fails.
pub fn drag_refs(
    from_ref: ElementRef,
    to_ref: ElementRef,
    app: &AppTarget,
    options: &DragOptions,
    from_cached: Option<IUIAutomationElement>,
    to_cached: Option<IUIAutomationElement>,
) -> Result<ActionResult, ForepawError> {
    app::activate_app(app)?;
    let from = snapshot::resolve_ref_position(from_ref.id, app, from_cached)?;
    let to = snapshot::resolve_ref_position(to_ref.id, app, to_cached)?;
    perform_mouse_drag(&[from, to], options)?;

    let start = window_relative(from, app);
    let end = window_relative(to, app);
    Ok(ActionResult::ok_msg(format!(
        "dragged from {:.0},{:.0} to {:.0},{:.0} ({} steps, {:.1}s)",
        start.x, start.y, end.x, end.y, options.steps, options.duration,
    )))
}

/// Interpolated mouse drag along `path` (screen-absolute, physical px). Holds
/// the button down between the first and last point, posting intermediate
/// `SetCursorPos` moves per step (Windows tracks the move as a drag while the
/// button is held). Modifiers are held for the whole drag.
#[expect(
    clippy::indexing_slicing,
    reason = "path indexing after len >= 2 check"
)]
fn perform_mouse_drag(path: &[Point], options: &DragOptions) -> Result<(), ForepawError> {
    if path.len() < 2 {
        return Ok(());
    }
    let first = path[0];
    let last = *path.last().expect("path has >= 2 elements (checked above)");
    let (down, up) = if options.right_button {
        (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
    } else {
        (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
    };

    // Hold modifiers for the entire drag.
    let mod_downs = modifier_inputs(&options.modifiers, false);
    let mod_ups = modifier_inputs(&options.modifiers, true);
    if !mod_downs.is_empty() {
        send_inputs(&mod_downs)?;
    }

    set_cursor_pos(first)?;
    send_inputs(&[mouse_input(down)])?;
    thread::sleep(Duration::from_millis(20));

    let segments = path.len() - 1;
    #[expect(
        clippy::cast_precision_loss,
        reason = "segment count to f64 for delay math"
    )]
    let step_delay = options.duration / (segments as f64) / f64::from(options.steps);
    // Move via relative SendInput events (injected input) rather than SetCursorPos
    // (which only synthesizes WM_MOUSEMOVE) so apps that only process real input
    // register the drag. Endpoints still use SetCursorPos for exact positioning.
    let mut prev = first;
    for seg_idx in 0..segments {
        let (seg_from, seg_to) = (path[seg_idx], path[seg_idx + 1]);
        for i in 1..=options.steps {
            let t = f64::from(i) / f64::from(options.steps);
            let point = Point::new(
                seg_from.x + (seg_to.x - seg_from.x) * t,
                seg_from.y + (seg_to.y - seg_from.y) * t,
            );
            #[expect(
                clippy::cast_possible_truncation,
                reason = "sub-pixel delta fits in i32"
            )]
            let dx = (point.x - prev.x).round() as i32;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "sub-pixel delta fits in i32"
            )]
            let dy = (point.y - prev.y).round() as i32;
            if dx != 0 || dy != 0 {
                send_inputs(&[mouse_move_input(dx, dy)])?;
            }
            prev = point;
            thread::sleep(Duration::from_secs_f64(step_delay));
        }
    }

    // Snap to the exact endpoint before release (relative-move rounding).
    set_cursor_pos(last)?;
    send_inputs(&[mouse_input(up)])?;
    if !mod_ups.is_empty() {
        send_inputs(&mod_ups)?;
    }
    Ok(())
}

/// Build key-down or key-up `INPUT`s for a set of modifiers. Key-ups are in
/// reverse order so nested chords release naturally.
fn modifier_inputs(modifiers: &[Modifier], up: bool) -> Vec<INPUT> {
    let flag = if up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    let mapped = modifiers.iter().filter_map(modifier_vk);
    if up {
        mapped.rev().map(|vk| keyboard_input(vk, 0, flag)).collect()
    } else {
        mapped.map(|vk| keyboard_input(vk, 0, flag)).collect()
    }
}

// ---------------------------------------------------------------------------
// SendInput plumbing
// ---------------------------------------------------------------------------

/// Build a keyboard `INPUT` struct.
fn keyboard_input(w_vk: u16, w_scan: u16, dw_flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(w_vk),
                wScan: w_scan,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Build a mouse button `INPUT` struct (no movement; pair with `SetCursorPos`
/// for positioning).
fn mouse_input(flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Build a mouse-wheel `INPUT` (no movement; pair with `move_mouse_to`).
/// `mouse_data` is the signed wheel delta packed into the DWORD field.
fn mouse_wheel_input(flags: MOUSE_EVENT_FLAGS, mouse_data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Build a relative mouse-move `INPUT` (no ABSOLUTE -- delta in pixels at
/// default mouse speed). Used for drag interpolation so the motion is injected
/// as real input, not a synthesized position.
fn mouse_move_input(dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Round a [`Point`] to integer pixels (screen coordinates are physical pixels).
fn to_pixels(point: &Point) -> (i32, i32) {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates fit in i32"
    )]
    let x = point.x.round() as i32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates fit in i32"
    )]
    let y = point.y.round() as i32;
    (x, y)
}

/// Submit a batch of `INPUT` events via `SendInput`, checking the return value.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `SendInput` inserts zero events.
fn send_inputs(inputs: &[INPUT]) -> Result<(), ForepawError> {
    if inputs.is_empty() {
        return Ok(());
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "INPUT size is a small compile-time constant"
    )]
    #[expect(
        clippy::cast_possible_wrap,
        reason = "INPUT size is a small positive constant"
    )]
    let cb_size = size_of::<INPUT>() as i32;
    // SAFETY: `inputs` is a valid slice of INPUT structs and `cb_size` is the
    // real struct size. SendInput reads exactly `inputs.len()` elements.
    let sent = unsafe { SendInput(inputs, cb_size) };
    if sent == 0 {
        return Err(ForepawError::ActionFailed(
            "SendInput inserted no events".into(),
        ));
    }
    Ok(())
}
