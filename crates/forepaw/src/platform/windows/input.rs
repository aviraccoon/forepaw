//! Input simulation via `SendInput` and `SetCursorPos` (keyboard, mouse,
//! and later scroll/drag).
//!
//! Keyboard and mouse-button events go through `SendInput`; cursor
//! positioning uses `SetCursorPos`, which takes physical pixels directly
//! (no 0..65535 normalization) and is correct at any DPI and on any monitor.

use std::mem::size_of;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};

use crate::core::errors::ForepawError;
use crate::core::key_combo::{ClickOptions, KeyCombo, MouseButton};
use crate::core::types::{Point, Rect};
use crate::platform::{ActionResult, AppTarget};

use super::app;
use super::key_code::{modifier_vk, virtual_key_code};

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

/// Move the cursor to a screen-absolute point (physical pixels) via
/// `SetCursorPos`. Physical pixels go in directly -- no 0..65535
/// normalization -- so this is correct at any DPI and on multi-monitor setups.
/// A 50ms settle mirrors the macOS backend.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `SetCursorPos` rejects the call.
fn move_mouse_to(point: Point) -> Result<(), ForepawError> {
    let (x, y) = to_pixels(&point);
    // SAFETY: SetCursorPos takes physical pixel coords; any in-range values are safe.
    unsafe { SetCursorPos(x, y) }
        .map_err(|e| ForepawError::ActionFailed(format!("SetCursorPos failed: {e}")))?;
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
    let center = Point::new(
        region.x + region.width / 2.0,
        region.y + region.height / 2.0,
    );
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
    let center = Point::new(
        region.x + region.width / 2.0,
        region.y + region.height / 2.0,
    );
    hover_at_point(center, Some(app), smooth)?;
    Ok(ActionResult::ok_msg(format!(
        "hovered region at {:.0},{:.0}",
        center.x, center.y
    )))
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
