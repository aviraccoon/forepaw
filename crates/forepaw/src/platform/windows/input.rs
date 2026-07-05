//! Input simulation via `SendInput` (keyboard, and later mouse/scroll/drag).
//!
//! All input is synthesized through Win32 `SendInput`, which injects events
//! into the calling thread's foreground window.

use std::mem::size_of;
use std::thread;
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY,
};

use crate::core::errors::ForepawError;
use crate::core::key_combo::KeyCombo;
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
