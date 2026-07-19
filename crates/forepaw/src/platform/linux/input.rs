//! Raw input injection via `/dev/uinput` (evdev).
//!
//! Creates a kernel virtual input device and emits evdev events directly via
//! `open`, `ioctl`, and `write` (no helper crate, no daemon). This is forepaw's
//! Linux input path, chosen over ydotool (AGPL, daemon) and EIS/libei
//! (KDE-only) for universality: uinput works on every compositor (wlroots, KDE,
//! GNOME) at the kernel level. The cost is `/dev/uinput` access (root or the
//! `uinput` group).
//!
//! # Coordinate vs. keyboard scope
//!
//! This module currently provides keyboard primitives only (`type_via_keyboard`,
//! `press`). Mouse primitives (absolute move, click, scroll, drag) are added
//! once the screen-geometry and window-global-position questions are resolved:
//! Wayland app windows expose surface-local bounds, so coordinate actions need
//! the compositor's view of window positions.
//!
//! # Recognition delay
//!
//! A freshly-created uinput device must be enumerated by libinput/KWin before
//! it accepts events — the first event emitted immediately after `UI_DEV_CREATE`
//! is dropped. [`UinputDevice::open`] sleeps a fixed floor after creation so the
//! device is functional. This is a correctness requirement (the device is inert
//! until bound), not an input-pacing policy; inter-action timing remains the
//! caller's concern (the library exposes direct primitives — pacing, settling,
//! and latency strategy belong to whoever drives the API).
//!
//! # Constants
//!
//! Event-type and key codes are verified against `linux/input-event-codes.h`
//! (linux-headers-6.18.7). The ioctl request codes are derived from struct
//! sizes via the asm-generic `_IOW` formula with compile-time `assert!`s, so
//! they cannot drift from the struct layout.

use std::io;
use std::io::Write;
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::core::encoder_detection::is_command_available;
use crate::core::errors::ForepawError;
use crate::core::key_combo::{DragOptions, KeyCombo, Modifier, MouseButton};
use crate::core::types::Point;

use super::compositor;
use super::key_code::{char_to_evdev, evdev_key_code, modifier_code, KeyStroke};

// --- evdev event codes (linux/input-event-codes.h, verified 6.18.7) ---
const EV_SYN: u16 = 0x00; // :39
const EV_KEY: u16 = 0x01; // :40
const EV_REL: u16 = 0x02; // :41
const EV_ABS: u16 = 0x03; // :42
const SYN_REPORT: u16 = 0; // :58
const ABS_X: u16 = 0x00; // :922
const ABS_Y: u16 = 0x01; // :923
const BTN_LEFT: u16 = 0x110; // :357
const BTN_RIGHT: u16 = 0x111; // :358
const REL_WHEEL: u16 = 0x08; // :943
const REL_HWHEEL: u16 = 0x06; // :841

// Highest evdev keycode advertised on the device. Every `KEY_*` used by typing
// or key combos (letters, digits, symbols, function keys, modifiers) is < 256.
const MAX_KEY_CODE: u16 = 0xff;

/// Sleep after `UI_DEV_CREATE` so libinput/KWin binds the device before the
/// first event (else it is dropped). See module docs.
///
/// A full-capability device (keyboard + absolute pointer + relative wheel)
/// takes longer to enumerate than a key-only one; 300ms is a safe floor. The
/// device is held for the session, so this is paid once.
const DEVICE_RECOGNITION_DELAY: Duration = Duration::from_millis(300);

/// Per-keystroke delay, matching the macOS/Windows backends: events arriving
/// too fast are dropped by some apps (Electron/Chromium).
const INTER_KEY_DELAY: Duration = Duration::from_millis(8);

const UINPUT_IOCTL_BASE: u32 = 0x55; // 'U' (UINPUT_IOCTL_BASE, uinput.h)

// --- struct layouts (repr(C), matching the kernel) ---

/// `struct input_id` (input.h:57): four `__u16`.
#[repr(C)]
#[derive(Clone, Copy)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

/// `struct uinput_setup` (uinput.h:67): `input_id` + `name[80]` + `ff_effects_max`.
const UINPUT_MAX_NAME_SIZE: usize = 80; // uinput.h:47

#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; UINPUT_MAX_NAME_SIZE],
    ff_effects_max: u32,
}

/// `struct input_event` (input.h:26): 24 bytes on 64-bit (timeval 16 + type/code + value).
#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}

// --- asm-generic ioctl encoding (asm-generic/ioctl.h) ---
// _IOC(dir,type,nr,size) = dir<<30 | type<<8 | nr | size<<16.

const IOC_NONE: u32 = 0;
const IOC_WRITE: u32 = 1;

#[must_use]
const fn ioc(dir: u32, type_: u32, nr: u32, size: u32) -> u32 {
    (dir << 30) | (type_ << 8) | nr | (size << 16)
}

// Pin the struct size the kernel's _IOW macro was defined against.
const _: () = assert!(size_of::<UinputSetup>() == 92);

/// `struct input_absinfo` (input.h:106): six `__s32`.
#[repr(C)]
#[derive(Clone, Copy)]
struct InputAbsinfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

/// `struct uinput_abs_setup` (uinput.h:81): `__u16 code` + (2-byte pad) +
/// `input_absinfo`.
#[repr(C)]
struct UinputAbsSetup {
    code: u16,
    absinfo: InputAbsinfo,
}
const _: () = assert!(size_of::<InputAbsinfo>() == 24);
const _: () = assert!(size_of::<UinputAbsSetup>() == 28);

// uinput request codes, derived from the _IOC formula + verified struct sizes.
const UI_DEV_CREATE: u32 = ioc(IOC_NONE, UINPUT_IOCTL_BASE, 1, 0); // _IO('U',1)
const UI_DEV_DESTROY: u32 = ioc(IOC_NONE, UINPUT_IOCTL_BASE, 2, 0); // _IO('U',2)
const UI_SET_EVBIT: u32 = ioc(IOC_WRITE, UINPUT_IOCTL_BASE, 100, 4); // _IOW('U',100,int)
const UI_SET_KEYBIT: u32 = ioc(IOC_WRITE, UINPUT_IOCTL_BASE, 101, 4); // _IOW('U',101,int)
const UI_SET_RELBIT: u32 = ioc(IOC_WRITE, UINPUT_IOCTL_BASE, 102, 4); // _IOW('U',102,int)
const UI_SET_ABSBIT: u32 = ioc(IOC_WRITE, UINPUT_IOCTL_BASE, 103, 4); // _IOW('U',103,int)
const UI_DEV_SETUP: u32 = ioc(IOC_WRITE, UINPUT_IOCTL_BASE, 3, 92); // _IOW('U',3,uinput_setup)
const UI_ABS_SETUP: u32 = ioc(IOC_WRITE, UINPUT_IOCTL_BASE, 4, 28); // _IOW('U',4,uinput_abs_setup)

/// A kernel virtual input device (`/dev/uinput`) for emitting evdev events.
///
/// Held lazily by [`super::LinuxProvider`] so it is created once (absorbing the
/// recognition delay under the first action's setup) and reused for the session.
#[derive(Debug)]
pub struct UinputDevice {
    fd: OwnedFd,
    /// Whether absolute-pointer capability was declared (requires the
    /// compositor's screen geometry at creation).
    has_pointer: bool,
}

impl UinputDevice {
    /// Open `/dev/uinput`, declare keyboard + pointer capabilities, create the
    /// device, and wait out the recognition delay.
    ///
    /// Pointer (absolute move + buttons + wheel) requires the compositor's
    /// screen geometry to set the absolute-axis range; on compositors where
    /// that's unavailable, `has_pointer` stays `false` and mouse actions will
    /// error (keyboard input still works).
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if `/dev/uinput` cannot be opened
    /// (needs root or the `uinput` group) or a kernel ioctl fails.
    pub fn open() -> Result<Self, ForepawError> {
        let fd = open_fd()?;
        // Keyboard capabilities (always available).
        set_bit(&fd, UI_SET_EVBIT, i32::from(EV_SYN))?;
        set_bit(&fd, UI_SET_EVBIT, i32::from(EV_KEY))?;
        for code in 1..=MAX_KEY_CODE {
            set_bit(&fd, UI_SET_KEYBIT, i32::from(code))?;
        }
        // Pointer capabilities: absolute move + buttons + relative wheel. The
        // absolute-axis range comes from the compositor's screen geometry
        // (physical pixels). libinput classifies an EV_ABS-only device as a
        // non-pointer, so EV_KEY + BTN_LEFT are required here even though the
        // keyboard section already declares EV_KEY.
        let has_pointer = match compositor::screen_geometry() {
            Ok(g) => {
                set_bit(&fd, UI_SET_EVBIT, i32::from(EV_ABS))?;
                set_bit(&fd, UI_SET_EVBIT, i32::from(EV_REL))?;
                set_bit(&fd, UI_SET_KEYBIT, i32::from(BTN_LEFT))?;
                set_bit(&fd, UI_SET_KEYBIT, i32::from(BTN_RIGHT))?;
                set_bit(&fd, UI_SET_ABSBIT, i32::from(ABS_X))?;
                set_bit(&fd, UI_SET_ABSBIT, i32::from(ABS_Y))?;
                set_bit(&fd, UI_SET_RELBIT, i32::from(REL_WHEEL))?;
                set_bit(&fd, UI_SET_RELBIT, i32::from(REL_HWHEEL))?;
                #[expect(clippy::cast_possible_truncation, reason = "screen pixels fit in i32")]
                let max_x = (g.width.round() as i32).max(1) - 1;
                #[expect(clippy::cast_possible_truncation, reason = "screen pixels fit in i32")]
                let max_y = (g.height.round() as i32).max(1) - 1;
                abs_setup(&fd, ABS_X, axis_range(max_x))?;
                abs_setup(&fd, ABS_Y, axis_range(max_y))?;
                true
            }
            Err(_) => false,
        };
        setup_device(&fd, "forepaw-uinput")?;
        ioctl0(&fd, UI_DEV_CREATE)?;
        thread::sleep(DEVICE_RECOGNITION_DELAY);
        Ok(Self { fd, has_pointer })
    }

    /// Emit a key event (`value` 1 = press, 0 = release) followed by a sync.
    fn key_event(&self, code: u16, down: bool) -> Result<(), ForepawError> {
        let value = i32::from(down);
        write_event(&self.fd, EV_KEY, code, value)?;
        write_event(&self.fd, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }

    /// Press and release a single evdev keycode, optionally holding Shift.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if a uinput write fails.
    pub fn keystroke(&self, stroke: KeyStroke) -> Result<(), ForepawError> {
        let shift = modifier_code(&Modifier::Shift);
        if stroke.shift {
            if let Some(shift_code) = shift {
                self.key_event(shift_code, true)?;
            }
        }
        self.key_event(stroke.code, true)?;
        thread::sleep(INTER_KEY_DELAY);
        self.key_event(stroke.code, false)?;
        if stroke.shift {
            if let Some(shift_code) = shift {
                self.key_event(shift_code, false)?;
            }
        }
        Ok(())
    }

    /// Press a key down (no release). Pair with [`Self::key_up`] for chords.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if a uinput write fails.
    pub fn key_down(&self, code: u16) -> Result<(), ForepawError> {
        self.key_event(code, true)
    }

    /// Release a key previously held with [`Self::key_down`].
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if a uinput write fails.
    pub fn key_up(&self, code: u16) -> Result<(), ForepawError> {
        self.key_event(code, false)
    }

    /// Whether the device has absolute-pointer capability.
    #[must_use]
    pub fn has_pointer(&self) -> bool {
        self.has_pointer
    }

    /// Move the pointer to a screen-absolute point (physical pixels) via an
    /// absolute event, then sync.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if the device has no pointer
    /// capability or a uinput write fails.
    pub fn move_to(&self, point: Point) -> Result<(), ForepawError> {
        if !self.has_pointer {
            return Err(ForepawError::ActionFailed(
                "uinput device has no pointer capability \
                 (compositor screen geometry unavailable)"
                    .into(),
            ));
        }
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
        write_event(&self.fd, EV_ABS, ABS_X, x)?;
        write_event(&self.fd, EV_ABS, ABS_Y, y)?;
        write_event(&self.fd, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }

    /// Press a mouse button down. Pair with [`Self::button_up`].
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if a uinput write fails.
    pub fn button_down(&self, button: MouseButton) -> Result<(), ForepawError> {
        self.button_event(pointer_button_code(button), true)
    }

    /// Release a mouse button previously held with [`Self::button_down`].
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if a uinput write fails.
    pub fn button_up(&self, button: MouseButton) -> Result<(), ForepawError> {
        self.button_event(pointer_button_code(button), false)
    }

    /// Emit a button event (`value` 1 = press, 0 = release) followed by a sync.
    fn button_event(&self, code: u16, down: bool) -> Result<(), ForepawError> {
        write_event(&self.fd, EV_KEY, code, i32::from(down))?;
        write_event(&self.fd, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }

    /// Scroll by `notches` (positive = up/right, negative = down/left).
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if a uinput write fails.
    pub fn wheel(&self, notches: i32, horizontal: bool) -> Result<(), ForepawError> {
        let axis = if horizontal { REL_HWHEEL } else { REL_WHEEL };
        write_event(&self.fd, EV_REL, axis, notches)?;
        write_event(&self.fd, EV_SYN, SYN_REPORT, 0)?;
        Ok(())
    }
}

impl Drop for UinputDevice {
    fn drop(&mut self) {
        // Best-effort destroy; the fd closes via OwnedFd regardless.
        let _destroy_result = ioctl0(&self.fd, UI_DEV_DESTROY);
    }
}

// ---------------------------------------------------------------------------
// Mouse action composites
// ---------------------------------------------------------------------------

/// Move to `point`, then click (button down/up) `count` times.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the device has no pointer or a
/// uinput write fails.
pub fn perform_click(
    dev: &UinputDevice,
    point: Point,
    button: MouseButton,
    count: u32,
) -> Result<(), ForepawError> {
    dev.move_to(point)?;
    thread::sleep(Duration::from_millis(30));
    for i in 1..=count {
        dev.button_down(button)?;
        thread::sleep(Duration::from_millis(20));
        dev.button_up(button)?;
        if i < count {
            thread::sleep(Duration::from_millis(40));
        }
    }
    Ok(())
}

/// Move to `target` via a short interpolated path so hover/enter handlers fire
/// (a single absolute teleport can miss them). Wayland exposes no cursor query,
/// so the path starts from a point offset from the target rather than the
/// actual current position. Ends with a dwell for hover timers.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the device has no pointer or a
/// uinput write fails.
pub fn hover_move(dev: &UinputDevice, target: Point) -> Result<(), ForepawError> {
    let start = Point::new((target.x - 40.0).max(0.0), (target.y - 40.0).max(0.0));
    let steps: u8 = 5;
    for i in 1..=steps {
        let t = f64::from(i) / f64::from(steps);
        dev.move_to(Point::new(
            start.x + (target.x - start.x) * t,
            start.y + (target.y - start.y) * t,
        ))?;
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(120));
    Ok(())
}

/// Drag along `path` (screen-absolute physical px). Holds the button from
/// the first point to the last, with `options.steps` interpolated moves per
/// segment at an even cadence. Modifiers are held for the whole drag and
/// released in reverse at the end. `options.pressure` is macOS-only and
/// ignored here. The caller validates path length and applies `close_path`.
///
/// On a `< 2`-element path this is a no-op early return, matching the
/// macOS/Windows primitives; all slice indexing below is in bounds after that
/// check.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the device has no pointer
/// capability or a uinput write fails.
#[expect(
    clippy::indexing_slicing,
    reason = "path indexing after len >= 2 check"
)]
pub fn perform_drag(
    dev: &UinputDevice,
    path: &[Point],
    options: &DragOptions,
) -> Result<(), ForepawError> {
    if path.len() < 2 {
        return Ok(());
    }
    let button = if options.right_button {
        MouseButton::Right
    } else {
        MouseButton::Left
    };
    let mods: Vec<u16> = options.modifiers.iter().filter_map(modifier_code).collect();

    for &m in &mods {
        dev.key_down(m)?;
    }

    let first = path[0];
    dev.move_to(first)?;
    thread::sleep(Duration::from_millis(20));
    dev.button_down(button)?;

    let segments = path.len() - 1;
    #[expect(
        clippy::cast_precision_loss,
        reason = "segment count to f64 for delay math"
    )]
    let step_delay = options.duration / (segments as f64) / f64::from(options.steps);
    for seg_idx in 0..segments {
        let (seg_from, seg_to) = (path[seg_idx], path[seg_idx + 1]);
        for i in 1..=options.steps {
            let t = f64::from(i) / f64::from(options.steps);
            dev.move_to(Point::new(
                seg_from.x + (seg_to.x - seg_from.x) * t,
                seg_from.y + (seg_to.y - seg_from.y) * t,
            ))?;
            thread::sleep(Duration::from_secs_f64(step_delay));
        }
    }

    // Snap to the exact endpoint (path[segments] == path.last(), both in bounds
    // after the len >= 2 early return) before release.
    dev.move_to(path[segments])?;
    dev.button_up(button)?;
    for &m in mods.iter().rev() {
        dev.key_up(m)?;
    }
    Ok(())
}

/// Map a [`MouseButton`] to its evdev code.
fn pointer_button_code(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => BTN_LEFT,
        MouseButton::Right => BTN_RIGHT,
    }
}

/// Build an `input_absinfo` spanning `0..=max` (value=0, zero fuzz/flat/flat/resolution).
fn axis_range(max: i32) -> InputAbsinfo {
    InputAbsinfo {
        value: 0,
        minimum: 0,
        maximum: max,
        fuzz: 0,
        flat: 0,
        resolution: 0,
    }
}

/// `ioctl(fd, UI_ABS_SETUP, &setup)` — configure an absolute-axis range.
fn abs_setup(fd: &OwnedFd, code: u16, absinfo: InputAbsinfo) -> Result<(), ForepawError> {
    let setup = UinputAbsSetup { code, absinfo };
    // SAFETY: UI_ABS_SETUP reads a uinput_abs_setup from userspace; fd is valid.
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), ioctl_req(UI_ABS_SETUP), &raw const setup) };
    if rc < 0 {
        return Err(ForepawError::ActionFailed(format!(
            "UI_ABS_SETUP failed: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(())
}

/// Type `text`, routing to the best available method and returning a short
/// description of what happened.
///
/// - Fully evdev-expressible text (printable ASCII) is typed via uinput
///   keycodes -- no clipboard side effects, per-character semantics preserved.
/// - Text with characters uinput can't express (non-ASCII) is pasted via the
///   clipboard (`wl-copy` + Ctrl+V) when `wl-copy` is available. This is
///   layout-independent and handles arbitrary Unicode; the previous clipboard
///   contents are restored afterward.
/// - If `wl-copy` is unavailable and the text has unexpressible characters,
///   falls back to uinput and skips them (noted in the returned message).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if a uinput write fails, `wl-copy`
/// fails to start, or the clipboard-paste keystroke fails.
pub fn type_text(dev: &UinputDevice, text: &str) -> Result<String, ForepawError> {
    let total = text.chars().count();
    let unexpressible = text.chars().filter(|c| char_to_evdev(*c).is_none()).count();

    if unexpressible == 0 {
        let typed = type_via_keyboard(dev, text)?;
        return Ok(format!("typed {typed} chars"));
    }
    if is_command_available("wl-copy") {
        type_via_clipboard(dev, text)?;
        return Ok(format!(
            "pasted {total} chars via clipboard (wl-copy + Ctrl+V; clipboard restored)"
        ));
    }
    let typed = type_via_keyboard(dev, text)?;
    Ok(format!(
        "typed {typed} of {total} chars ({unexpressible} non-ASCII skipped; \
         install wl-clipboard for Unicode support)"
    ))
}

/// Paste `text` via the clipboard: save the current selection, copy `text` to
/// the clipboard, emit Ctrl+V, then restore the previous contents.
///
/// Reaches arbitrary Unicode (the compositor takes the clipboard string
/// verbatim), unlike uinput's positional keycodes. Requires `wl-copy`/
/// `wl-paste` (the `wl-clipboard` package).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `wl-copy` fails to start or the
/// paste keystroke fails.
fn type_via_clipboard(dev: &UinputDevice, text: &str) -> Result<(), ForepawError> {
    // Capture the current clipboard so we can put it back.
    let saved = Command::new("wl-paste")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| o.stdout);

    copy_to_clipboard(text.as_bytes())?;
    // Let the wl-copy daemon finish claiming the selection from the previous
    // owner before the paste reads it (the handover is a Wayland round-trip;
    // too short and the paste reads the *old* contents).
    thread::sleep(Duration::from_millis(250));

    // Paste via uinput Ctrl+V (the target app must already be focused).
    paste(dev)?;
    // Let the target finish reading the clipboard before we restore (overwriting
    // the selection mid-read would hand the app the wrong text).
    thread::sleep(Duration::from_millis(200));

    // Restore the previous contents, or clear the clipboard if we couldn't
    // capture what was there.
    match saved {
        Some(bytes) => {
            let _restored = copy_to_clipboard(&bytes);
        }
        None => {
            let _cleared = Command::new("wl-copy").arg("--clear").status();
        }
    }
    Ok(())
}

/// Emit a Ctrl+V chord (paste) via uinput.
fn paste(dev: &UinputDevice) -> Result<(), ForepawError> {
    let Some(ctrl) = modifier_code(&Modifier::Control) else {
        return Err(ForepawError::ActionFailed(
            "Control modifier has no evdev code".into(),
        ));
    };
    let Some(v) = evdev_key_code("v") else {
        return Err(ForepawError::ActionFailed("'v' has no evdev code".into()));
    };
    dev.key_down(ctrl)?;
    dev.key_down(v)?;
    dev.key_up(v)?;
    dev.key_up(ctrl)?;
    Ok(())
}

/// Write `bytes` to the clipboard via `wl-copy` (reads stdin until EOF).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `wl-copy` fails to spawn.
fn copy_to_clipboard(bytes: &[u8]) -> Result<(), ForepawError> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| ForepawError::ActionFailed(format!("wl-copy failed to start: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _written = stdin.write_all(bytes);
    }
    // stdin dropped here -> wl-copy sees EOF and accepts the text.
    let _waited = child.wait();
    Ok(())
}

// ---------------------------------------------------------------------------
// Keyboard actions
// ---------------------------------------------------------------------------

/// Type a string character-by-character via evdev keycodes (US QWERTY layout).
///
/// Returns the number of characters actually emitted. Characters that can't be
/// expressed as positional keycodes (non-ASCII) are skipped — a clipboard-paste
/// fallback for Unicode is deferred (see `key_code::char_to_evdev` docs).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if a uinput write fails.
pub fn type_via_keyboard(dev: &UinputDevice, text: &str) -> Result<u32, ForepawError> {
    let mut typed = 0_u32;
    for c in text.chars() {
        if let Some(stroke) = char_to_evdev(c) {
            dev.keystroke(stroke)?;
            typed += 1;
        }
    }
    Ok(typed)
}

/// Press a key combo (modifiers + key) as a single chord: modifier key-downs,
/// main key down/up, modifier key-ups in reverse.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the key name is unrecognized or a
/// uinput write fails.
pub fn press(dev: &UinputDevice, combo: &KeyCombo) -> Result<(), ForepawError> {
    let key = evdev_key_code(&combo.key)
        .ok_or_else(|| ForepawError::ActionFailed(format!("unknown key: '{}'", combo.key)))?;
    let mods: Vec<u16> = combo.modifiers.iter().filter_map(modifier_code).collect();

    for &m in &mods {
        dev.key_down(m)?;
    }
    dev.key_down(key)?;
    dev.key_up(key)?;
    for &m in mods.iter().rev() {
        dev.key_up(m)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// uinput ioctl plumbing
// ---------------------------------------------------------------------------

fn open_fd() -> Result<OwnedFd, ForepawError> {
    // SAFETY: open() on a NUL-terminated fixed path returns a valid fd or -1.
    let raw = unsafe { libc::open(c"/dev/uinput".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    if raw < 0 {
        return Err(ForepawError::ActionFailed(format!(
            "cannot open /dev/uinput: {} (needs root or the uinput group)",
            io::Error::last_os_error()
        )));
    }
    // SAFETY: `raw` was just opened; OwnedFd takes ownership for RAII close.
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}

/// `ioctl(fd, request, &bit)` for the `UI_SET_*` family.
fn set_bit(fd: &OwnedFd, request: u32, bit: i32) -> Result<(), ForepawError> {
    // SAFETY: UI_SET_* take a single int argument; fd is a valid uinput fd.
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), ioctl_req(request), bit) };
    if rc < 0 {
        Err(ForepawError::ActionFailed(format!(
            "uinput set bit {bit} failed: {}",
            io::Error::last_os_error()
        )))
    } else {
        Ok(())
    }
}

/// `ioctl(fd, UI_DEV_SETUP, &setup)`.
fn setup_device(fd: &OwnedFd, name: &str) -> Result<(), ForepawError> {
    let mut setup = UinputSetup {
        id: InputId {
            bustype: 0x0003, // BUS_USB
            vendor: 0x1234,
            product: 0x5678,
            version: 1,
        },
        name: [0; UINPUT_MAX_NAME_SIZE],
        ff_effects_max: 0,
    };
    for (dst, src) in setup
        .name
        .iter_mut()
        .zip(name.as_bytes().iter().take(UINPUT_MAX_NAME_SIZE - 1))
    {
        *dst = *src;
    }
    // SAFETY: UI_DEV_SETUP reads a uinput_setup from userspace; fd is valid.
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), ioctl_req(UI_DEV_SETUP), &raw mut setup) };
    if rc < 0 {
        Err(ForepawError::ActionFailed(format!(
            "UI_DEV_SETUP failed: {}",
            io::Error::last_os_error()
        )))
    } else {
        Ok(())
    }
}

/// Argument-less ioctl (`UI_DEV_CREATE` / `UI_DEV_DESTROY`).
fn ioctl0(fd: &OwnedFd, request: u32) -> Result<(), ForepawError> {
    // SAFETY: these requests take no argument; fd is a valid uinput fd.
    let rc = unsafe { libc::ioctl(fd.as_raw_fd(), ioctl_req(request)) };
    if rc < 0 {
        Err(ForepawError::ActionFailed(format!(
            "uinput ioctl {request:#x} failed: {}",
            io::Error::last_os_error()
        )))
    } else {
        Ok(())
    }
}

/// `write()` a single `input_event` (timestamp zeroed; the kernel stamps it).
fn write_event(fd: &OwnedFd, kind: u16, code: u16, value: i32) -> Result<(), ForepawError> {
    let event = InputEvent {
        time: libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        kind,
        code,
        value,
    };
    // SAFETY: writes `size_of::<InputEvent>()` bytes from a valid local.
    let written = unsafe {
        libc::write(
            fd.as_raw_fd(),
            (&raw const event).cast::<libc::c_void>(),
            size_of::<InputEvent>(),
        )
    };
    if written < 0 {
        Err(ForepawError::ActionFailed(format!(
            "uinput write failed: {}",
            io::Error::last_os_error()
        )))
    } else {
        Ok(())
    }
}

/// Widen the computed u32 request code to the libc `ioctl` request type.
/// On musl this is `c_int` (glibc: `c_ulong`); the value always fits i32.
fn ioctl_req(request: u32) -> libc::Ioctl {
    #[expect(
        clippy::cast_possible_wrap,
        reason = "ioctl request codes are < 2^31 (dir field is bits 30-31)"
    )]
    let r: libc::Ioctl = request as libc::Ioctl;
    r
}
