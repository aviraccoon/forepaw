//! Input simulation via CGEvent (mouse, keyboard, scroll, drag, hover).
//!
//! All input is synthesized through CoreGraphics events posted at the HID event tap.

use std::thread;
use std::time::Duration;

use crate::core::errors::ForepawError;
use crate::core::key_combo::{ClickOptions, DragOptions, KeyCombo, Modifier, MouseButton};
use crate::core::types::Point;
use crate::platform::darwin::app::{self, ResolvedWindow};
use crate::platform::darwin::ffi::{self, CGPointFFI, CGRectFFI};
use crate::platform::darwin::key_code;
use crate::platform::darwin::snapshot;
use crate::platform::ActionResult;

use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

/// Activate an app and wait for it to come to the foreground.
pub fn activate_app(app_name: &str) -> Result<(Retained<NSRunningApplication>, i32), ForepawError> {
    let running_app = app::find_app(app_name)?;
    let pid = running_app.processIdentifier();
    #[allow(deprecated)]
    running_app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    thread::sleep(Duration::from_millis(300));
    Ok((running_app, pid))
}

/// Activate an app and resolve its main window.
fn activate_and_resolve_window(
    app_name: &str,
    window: Option<&str>,
) -> Result<(Retained<NSRunningApplication>, i32, ResolvedWindow), ForepawError> {
    let (running_app, pid) = activate_app(app_name)?;
    let resolved = app::find_window(pid, window)?;
    Ok((running_app, pid, resolved))
}

// ---------------------------------------------------------------------------
// Mouse
// ---------------------------------------------------------------------------

/// Post a mouse event at the given screen point.
unsafe fn post_mouse_event(
    event_type: u32,
    point: CGPointFFI,
    button: u32,
    click_state: Option<i64>,
) -> Result<(), ForepawError> {
    let event = ffi::CGEventCreateMouseEvent(std::ptr::null_mut(), event_type, point, button);
    if event.is_null() {
        return Err(ForepawError::ActionFailed(
            "failed to create mouse event".into(),
        ));
    }
    if let Some(count) = click_state {
        ffi::CGEventSetIntegerValueField(event, ffi::K_CG_MOUSE_EVENT_CLICK_STATE, count);
    }
    ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, event);
    ffi::CFRelease(event as ffi::CFTypeRef);
    Ok(())
}

/// Move the cursor to a screen point (teleport, no intermediate events).
pub fn move_mouse_to(point: CGPointFFI) -> Result<(), ForepawError> {
    unsafe {
        post_mouse_event(
            ffi::K_CG_EVENT_MOUSE_MOVED,
            point,
            ffi::K_CG_MOUSE_BUTTON_LEFT,
            None,
        )?;
    }
    thread::sleep(Duration::from_millis(50));
    Ok(())
}

/// Move the cursor smoothly from its current position to the target.
/// Posts intermediate mouseMoved events so hover handlers fire.
fn smooth_move_mouse(
    target: CGPointFFI,
    steps: usize,
    duration: Duration,
) -> Result<(), ForepawError> {
    let current = unsafe {
        let locator_event = ffi::CGEventCreateMouseEvent(
            std::ptr::null_mut(),
            ffi::K_CG_EVENT_MOUSE_MOVED,
            CGPointFFI { x: 0.0, y: 0.0 },
            ffi::K_CG_MOUSE_BUTTON_LEFT,
        );
        if locator_event.is_null() {
            target
        } else {
            let loc = ffi::CGEventGetLocation(locator_event);
            ffi::CFRelease(locator_event as ffi::CFTypeRef);
            loc
        }
    };

    let step_delay = duration / steps as u32;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = current.x + (target.x - current.x) * t;
        let y = current.y + (target.y - current.y) * t;
        let point = CGPointFFI { x, y };
        unsafe {
            post_mouse_event(
                ffi::K_CG_EVENT_MOUSE_MOVED,
                point,
                ffi::K_CG_MOUSE_BUTTON_LEFT,
                None,
            )?;
        }
        thread::sleep(step_delay);
    }
    Ok(())
}

/// Perform a mouse click at a screen point.
pub fn perform_mouse_click(
    point: CGPointFFI,
    button: MouseButton,
    click_count: u32,
) -> Result<(), ForepawError> {
    // Move to target first so the click routes to the right window
    move_mouse_to(point)?;

    let (down_type, up_type, cg_button) = match button {
        MouseButton::Right => (
            ffi::K_CG_EVENT_RIGHT_MOUSE_DOWN,
            ffi::K_CG_EVENT_RIGHT_MOUSE_UP,
            ffi::K_CG_MOUSE_BUTTON_RIGHT,
        ),
        MouseButton::Left => (
            ffi::K_CG_EVENT_LEFT_MOUSE_DOWN,
            ffi::K_CG_EVENT_LEFT_MOUSE_UP,
            ffi::K_CG_MOUSE_BUTTON_LEFT,
        ),
    };

    for i in 1..=click_count {
        unsafe {
            post_mouse_event(down_type, point, cg_button, Some(i64::from(i)))?;
            post_mouse_event(up_type, point, cg_button, Some(i64::from(i)))?;
        }
        if i < click_count {
            thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

/// Click an AXUIElement. Tries AXPress first, falls back to mouse click at center.
pub fn click_element(
    element: ffi::AXUIElementRef,
    options: &ClickOptions,
    pid: Option<i32>,
) -> Result<ActionResult, ForepawError> {
    let role = snapshot::get_ax_string_attr(element, "AXRole");
    let is_right_click = options.button == MouseButton::Right;
    let is_double_click = options.click_count > 1;

    // For web content links, prefer mouse click -- AXPress often doesn't
    // trigger navigation in browsers. For right-click/double-click, always use mouse.
    let prefer_mouse = role.as_deref() == Some("AXLink") || is_right_click || is_double_click;

    if !prefer_mouse {
        // Try AXPress first
        let action = unsafe {
            let action_str = app::cf_string_from_str("AXPress");
            let result = ffi::AXUIElementPerformAction(element, action_str);
            ffi::CFRelease(action_str as ffi::CFTypeRef);
            result
        };
        if action == ffi::AXError::Success {
            return Ok(ActionResult::ok_msg("pressed via AX"));
        }
    }

    // Mouse click at element center
    if let (Some(pos), Some((w, h))) = (
        snapshot::get_element_position(element),
        snapshot::get_element_size(element),
    ) {
        let screen_point = CGPointFFI {
            x: pos.x + w / 2.0,
            y: pos.y + h / 2.0,
        };

        let mouse_button = if is_right_click {
            MouseButton::Right
        } else {
            MouseButton::Left
        };
        perform_mouse_click(screen_point, mouse_button, options.click_count)?;

        // Report window-relative coordinates
        let window_origin = pid
            .and_then(|p| app::find_window(p, None).ok())
            .map_or(Point::new(0.0, 0.0), |w| w.origin());
        let rel_x = (screen_point.x - window_origin.x) as i32;
        let rel_y = (screen_point.y - window_origin.y) as i32;
        let label = if is_right_click {
            "right-clicked"
        } else if is_double_click {
            "double-clicked"
        } else {
            "clicked"
        };
        return Ok(ActionResult::ok_msg(format!("{label} at {rel_x},{rel_y}")));
    }

    // Last resort for links: try AXPress anyway (only for regular left click)
    if !is_right_click && !is_double_click && prefer_mouse {
        let action = unsafe {
            let action_str = app::cf_string_from_str("AXPress");
            let result = ffi::AXUIElementPerformAction(element, action_str);
            ffi::CFRelease(action_str as ffi::CFTypeRef);
            result
        };
        if action == ffi::AXError::Success {
            return Ok(ActionResult::ok_msg("pressed via AX (fallback)"));
        }
    }

    Ok(ActionResult::fail(
        "click failed: no position and AXPress unsuccessful",
    ))
}

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

/// Type a string character-by-character via CGEvent keyboard events.
/// Inter-character delay is essential for Electron apps (Discord, Slack)
/// which drop characters if events arrive too fast.
pub fn type_via_keyboard(text: &str) -> Result<(), ForepawError> {
    for ch in text.chars() {
        let mut utf16_buf = [0_u16; 2];
        let utf16 = ch.encode_utf16(&mut utf16_buf);
        unsafe {
            // Key down
            let key_down = ffi::CGEventCreateKeyboardEvent(
                std::ptr::null_mut(),
                0, // virtual key doesn't matter for unicode input
                1,
            );
            if !key_down.is_null() {
                ffi::CGEventKeyboardSetUnicodeString(key_down, utf16.len() as u32, utf16.as_ptr());
                ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, key_down);
                ffi::CFRelease(key_down as ffi::CFTypeRef);
            }

            // Key up
            let key_up = ffi::CGEventCreateKeyboardEvent(std::ptr::null_mut(), 0, 0);
            if !key_up.is_null() {
                ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, key_up);
                ffi::CFRelease(key_up as ffi::CFTypeRef);
            }
        }
        thread::sleep(Duration::from_millis(8));
    }
    Ok(())
}

/// Set a value on an AX element. Tries AXValue first, falls back to
/// AXRaise + AXFocus + keyboard type.
pub fn set_value_on_element(
    element: ffi::AXUIElementRef,
    value: &str,
) -> Result<ActionResult, ForepawError> {
    let cf_value = {
        let ns_string = objc2_foundation::NSString::from_str(value);
        let cf_str = objc2::rc::Retained::as_ptr(&ns_string) as ffi::CFTypeRef;
        std::mem::forget(ns_string);
        cf_str
    };

    let result = unsafe {
        let attr = app::cf_string_from_str("AXValue");
        let r = ffi::AXUIElementSetAttributeValue(element, attr, cf_value);
        ffi::CFRelease(attr as ffi::CFTypeRef);
        ffi::CFRelease(cf_value);
        r
    };

    if result == ffi::AXError::Success {
        return Ok(ActionResult::ok());
    }

    // Fallback: focus and type via keyboard
    unsafe {
        let raise_action = app::cf_string_from_str("AXRaise");
        let _ = ffi::AXUIElementPerformAction(element, raise_action);
        ffi::CFRelease(raise_action as ffi::CFTypeRef);

        let focus_attr = app::cf_string_from_str("AXFocused");
        let cf_true = ffi::kCFBooleanTrue;
        let _ = ffi::AXUIElementSetAttributeValue(element, focus_attr, cf_true);
        ffi::CFRelease(focus_attr as ffi::CFTypeRef);
    }

    type_via_keyboard(value)?;
    Ok(ActionResult::ok_msg("typed via keyboard simulation"))
}

/// Press a key combo (modifiers + key) via CGEvent.
pub fn press_via_keyboard(combo: &KeyCombo) -> Result<(), ForepawError> {
    let key_code = key_code::virtual_key_code(&combo.key).unwrap_or(0);
    let flags = modifier_flags(&combo.modifiers);

    unsafe {
        let key_down = ffi::CGEventCreateKeyboardEvent(std::ptr::null_mut(), key_code, 1);
        let key_up = ffi::CGEventCreateKeyboardEvent(std::ptr::null_mut(), key_code, 0);
        if key_down.is_null() || key_up.is_null() {
            return Err(ForepawError::ActionFailed(
                "failed to create keyboard events".into(),
            ));
        }
        ffi::CGEventSetFlags(key_down, flags);
        ffi::CGEventSetFlags(key_up, flags);
        ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, key_down);
        ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, key_up);
        ffi::CFRelease(key_down as ffi::CFTypeRef);
        ffi::CFRelease(key_up as ffi::CFTypeRef);
    }
    Ok(())
}

/// Convert modifiers to CGEventFlags bitmask.
fn modifier_flags(modifiers: &[Modifier]) -> u64 {
    let mut flags: u64 = 0;
    for m in modifiers {
        match m {
            Modifier::Command => flags |= ffi::K_CG_EVENT_FLAG_CMD,
            Modifier::Shift => flags |= ffi::K_CG_EVENT_FLAG_SHIFT,
            Modifier::Option => flags |= ffi::K_CG_EVENT_FLAG_ALT,
            Modifier::Control => flags |= ffi::K_CG_EVENT_FLAG_CTRL,
        }
    }
    flags
}

// ---------------------------------------------------------------------------
// Scroll
// ---------------------------------------------------------------------------

/// Post a scroll wheel event at the current mouse position.
fn post_scroll_event(delta_y: i32, delta_x: i32) -> Result<(), ForepawError> {
    unsafe {
        let event = ffi::CGEventCreateScrollWheelEvent(
            std::ptr::null_mut(),
            ffi::K_CG_SCROLL_EVENT_UNIT_LINE,
            2, // wheelCount
            delta_y,
            delta_x,
            0,
        );
        if event.is_null() {
            return Err(ForepawError::ActionFailed(
                "failed to create scroll event".into(),
            ));
        }
        ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, event);
        ffi::CFRelease(event as ffi::CFTypeRef);
    }
    Ok(())
}

/// Move the mouse to the scroll target point before scrolling.
/// This ensures hover effects are present for boundary detection.
fn move_mouse_to_scroll_target(point: CGPointFFI) {
    let _ = unsafe {
        post_mouse_event(
            ffi::K_CG_EVENT_MOUSE_MOVED,
            point,
            ffi::K_CG_MOUSE_BUTTON_LEFT,
            None,
        )
    };
    thread::sleep(Duration::from_millis(50));
}

/// Capture a pixel fingerprint of a window for scroll boundary detection.
/// Uses CGWindowListCreateImage to grab a thin horizontal strip from the
/// window center -- fast, no file I/O.
fn capture_scroll_fingerprint(window_id: u32) -> Option<Vec<u8>> {
    unsafe {
        let image = ffi::CGWindowListCreateImage(
            CGRectFFI {
                origin: CGPointFFI { x: 0.0, y: 0.0 },
                size: ffi::CGSizeFFI {
                    width: 0.0,
                    height: 0.0,
                },
            },
            ffi::CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY,
            window_id,
            ffi::CG_WINDOW_IMAGE_NOMINAL_RESOLUTION | ffi::CG_WINDOW_IMAGE_BOUNDS_IGNORE_FRAMING,
        );
        if image.is_null() {
            return None;
        }

        let h = ffi::CGImageGetHeight(image) as i32;
        let w = ffi::CGImageGetWidth(image) as i32;
        if h <= 40 || w <= 0 {
            ffi::CFRelease(image as ffi::CFTypeRef);
            return None;
        }

        // Crop a 20px tall strip from the vertical center, excluding the
        // rightmost 30px to avoid transient scrollbar overlays.
        let strip_y = (h / 2 - 10) as usize;
        let strip_w = std::cmp::max(1, w - 30) as usize;
        let strip_rect = CGRectFFI {
            origin: CGPointFFI {
                x: 0.0,
                y: strip_y as f64,
            },
            size: ffi::CGSizeFFI {
                width: strip_w as f64,
                height: 20.0,
            },
        };

        let strip = ffi::CGImageCreateWithImageInRect(image, strip_rect);
        if strip.is_null() {
            ffi::CFRelease(image as ffi::CFTypeRef);
            return None;
        }

        // Get pixel data from the strip. We need a bitmap context.
        let strip_w = ffi::CGImageGetWidth(strip);
        let strip_h = ffi::CGImageGetHeight(strip);
        let color_space = ffi::CGColorSpaceCreateDeviceRGB();
        let bytes_per_row = strip_w * 4;
        let mut data = vec![0_u8; bytes_per_row * strip_h];

        let ctx = ffi::CGBitmapContextCreate(
            data.as_mut_ptr() as *mut std::ffi::c_void,
            strip_w,
            strip_h,
            8,
            bytes_per_row,
            color_space,
            1 << 1 | 1 << 2, // kCGImageAlphaPremultipliedLast | kCGImageByteOrder32Little
        );

        if ctx.is_null() {
            ffi::CFRelease(strip as ffi::CFTypeRef);
            ffi::CFRelease(image as ffi::CFTypeRef);
            ffi::CFRelease(color_space as ffi::CFTypeRef);
            return None;
        }

        ffi::CGContextDrawImage(
            ctx,
            CGRectFFI {
                origin: CGPointFFI { x: 0.0, y: 0.0 },
                size: ffi::CGSizeFFI {
                    width: strip_w as f64,
                    height: strip_h as f64,
                },
            },
            strip,
        );

        ffi::CFRelease(ctx as ffi::CFTypeRef);
        ffi::CFRelease(strip as ffi::CFTypeRef);
        ffi::CFRelease(image as ffi::CFTypeRef);
        ffi::CFRelease(color_space as ffi::CFTypeRef);

        Some(data)
    }
}

// ---------------------------------------------------------------------------
// Drag
// ---------------------------------------------------------------------------

/// Perform a mouse drag along a path of screen points.
fn perform_mouse_drag(path: &[CGPointFFI], options: &DragOptions) -> Result<(), ForepawError> {
    if path.len() < 2 {
        return Ok(());
    }
    let first = path[0];
    let last = *path.last().unwrap();

    let (down_type, drag_type, up_type, cg_button) = if options.right_button {
        (
            ffi::K_CG_EVENT_RIGHT_MOUSE_DOWN,
            ffi::K_CG_EVENT_RIGHT_MOUSE_DRAGGED,
            ffi::K_CG_EVENT_RIGHT_MOUSE_UP,
            ffi::K_CG_MOUSE_BUTTON_RIGHT,
        )
    } else {
        (
            ffi::K_CG_EVENT_LEFT_MOUSE_DOWN,
            ffi::K_CG_EVENT_LEFT_MOUSE_DRAGGED,
            ffi::K_CG_EVENT_LEFT_MOUSE_UP,
            ffi::K_CG_MOUSE_BUTTON_LEFT,
        )
    };

    let flags = modifier_flags(&options.modifiers);

    // Move to start
    move_mouse_to(first)?;

    // Mouse down
    unsafe {
        let mouse_down =
            ffi::CGEventCreateMouseEvent(std::ptr::null_mut(), down_type, first, cg_button);
        if mouse_down.is_null() {
            return Err(ForepawError::ActionFailed(
                "failed to create mouseDown event".into(),
            ));
        }
        if flags != 0 {
            ffi::CGEventSetFlags(mouse_down, flags);
        }
        if let Some(pressure) = options.pressure {
            ffi::CGEventSetDoubleValueField(mouse_down, ffi::K_CG_MOUSE_EVENT_PRESSURE, pressure);
        }
        ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, mouse_down);
        ffi::CFRelease(mouse_down as ffi::CFTypeRef);
    }
    thread::sleep(Duration::from_millis(20));

    // Drag through segments
    let segments = path.len() - 1;
    let segment_duration = options.duration / segments as f64;
    let step_delay = segment_duration / f64::from(options.steps);

    for seg_idx in 0..segments {
        let seg_from = path[seg_idx];
        let seg_to = path[seg_idx + 1];
        for i in 1..=options.steps {
            let t = f64::from(i) / f64::from(options.steps);
            let point = CGPointFFI {
                x: seg_from.x + (seg_to.x - seg_from.x) * t,
                y: seg_from.y + (seg_to.y - seg_from.y) * t,
            };

            unsafe {
                let drag_event =
                    ffi::CGEventCreateMouseEvent(std::ptr::null_mut(), drag_type, point, cg_button);
                if drag_event.is_null() {
                    continue;
                }
                if flags != 0 {
                    ffi::CGEventSetFlags(drag_event, flags);
                }
                if let Some(pressure) = options.pressure {
                    ffi::CGEventSetDoubleValueField(
                        drag_event,
                        ffi::K_CG_MOUSE_EVENT_PRESSURE,
                        pressure,
                    );
                }
                ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, drag_event);
                ffi::CFRelease(drag_event as ffi::CFTypeRef);
            }
            thread::sleep(Duration::from_secs_f64(step_delay));
        }
    }

    // Mouse up
    unsafe {
        let mouse_up = ffi::CGEventCreateMouseEvent(std::ptr::null_mut(), up_type, last, cg_button);
        if mouse_up.is_null() {
            return Err(ForepawError::ActionFailed(
                "failed to create mouseUp event".into(),
            ));
        }
        if flags != 0 {
            ffi::CGEventSetFlags(mouse_up, flags);
        }
        if let Some(pressure) = options.pressure {
            ffi::CGEventSetDoubleValueField(mouse_up, ffi::K_CG_MOUSE_EVENT_PRESSURE, pressure);
        }
        ffi::CGEventPost(ffi::K_CG_EVENT_TAP_CGHID, mouse_up);
        ffi::CFRelease(mouse_up as ffi::CFTypeRef);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Public API (called from mod.rs DesktopProvider impl)
// ---------------------------------------------------------------------------

/// Click a ref in a specific app.
pub fn click_ref(
    r#ref: crate::core::element_tree::ElementRef,
    app_name: &str,
    options: &ClickOptions,
) -> Result<ActionResult, ForepawError> {
    let (_, pid) = activate_app(app_name)?;
    let element = snapshot::resolve_ref_element(r#ref.id, app_name)?;
    click_element(element, options, Some(pid))
}

/// Click at a screen point (window-relative when app is specified).
pub fn click_at_point(
    point: Point,
    app_name: &str,
    options: &ClickOptions,
) -> Result<ActionResult, ForepawError> {
    let (_, pid, _resolved) = activate_and_resolve_window(app_name, None)?;
    app::validate_point_in_window(&point, pid)?;
    let screen_point = app::to_screen_point(&point, pid)?;
    let cg_point = CGPointFFI {
        x: screen_point.x,
        y: screen_point.y,
    };

    perform_mouse_click(cg_point, options.button, options.click_count)?;

    let rel_x = point.x as i32;
    let rel_y = point.y as i32;
    let label = match options.button {
        MouseButton::Right => "right-clicked",
        MouseButton::Left if options.click_count > 1 => "double-clicked",
        MouseButton::Left => "clicked",
    };
    Ok(ActionResult::ok_msg(format!("{label} at {rel_x},{rel_y}")))
}

/// Click at the center of a region (saliency-detected area).
pub fn click_region(
    region: crate::core::types::Rect,
    app_name: &str,
    window: Option<&str>,
    options: &ClickOptions,
) -> Result<ActionResult, ForepawError> {
    let (_, pid, _resolved) = activate_and_resolve_window(app_name, window)?;
    let center = Point::new(
        region.x + region.width / 2.0,
        region.y + region.height / 2.0,
    );
    app::validate_point_in_window(&center, pid)?;
    let screen_point = app::to_screen_point(&center, pid)?;
    let cg_point = CGPointFFI {
        x: screen_point.x,
        y: screen_point.y,
    };

    perform_mouse_click(cg_point, options.button, options.click_count)?;

    let rel_x = center.x as i32;
    let rel_y = center.y as i32;
    Ok(ActionResult::ok_msg(format!(
        "clicked region at {rel_x},{rel_y}"
    )))
}

/// Hover over an element ref.
pub fn hover_ref(
    r#ref: crate::core::element_tree::ElementRef,
    app_name: &str,
) -> Result<ActionResult, ForepawError> {
    let (_, pid) = activate_app(app_name)?;
    let element = snapshot::resolve_ref_element(r#ref.id, app_name)?;

    let pos = snapshot::get_element_position(element)
        .ok_or_else(|| ForepawError::ActionFailed(format!("Cannot determine position of {ref}")))?;
    let (w, h) = snapshot::get_element_size(element)
        .ok_or_else(|| ForepawError::ActionFailed(format!("Cannot determine size of {ref}")))?;

    let screen_point = CGPointFFI {
        x: pos.x + w / 2.0,
        y: pos.y + h / 2.0,
    };
    move_mouse_to(screen_point)?;

    // Report window-relative coordinates
    let window_origin = app::find_window(pid, None).map_or(Point::new(0.0, 0.0), |w| w.origin());
    let rel_x = (screen_point.x - window_origin.x) as i32;
    let rel_y = (screen_point.y - window_origin.y) as i32;
    Ok(ActionResult::ok_msg(format!("hovered at {rel_x},{rel_y}")))
}

/// Hover at a point. If app is specified, coordinates are window-relative.
/// Otherwise screen-absolute.
pub fn hover_at_point(
    point: Point,
    app_name: Option<&str>,
    smooth: bool,
) -> Result<ActionResult, ForepawError> {
    let target = if let Some(app_name) = app_name {
        let (_, pid) = activate_app(app_name)?;
        app::validate_point_in_window(&point, pid)?;
        let screen = app::to_screen_point(&point, pid)?;
        CGPointFFI {
            x: screen.x,
            y: screen.y,
        }
    } else {
        CGPointFFI {
            x: point.x,
            y: point.y,
        }
    };

    if smooth {
        smooth_move_mouse(target, 20, Duration::from_millis(150))?;
    } else {
        move_mouse_to(target)?;
    }
    Ok(ActionResult::ok_msg(format!(
        "hovered at {},{}",
        point.x as i32, point.y as i32
    )))
}

/// Hover at the center of a region.
pub fn hover_region(
    region: crate::core::types::Rect,
    app_name: &str,
    _window: Option<&str>,
    smooth: bool,
) -> Result<ActionResult, ForepawError> {
    let center = Point::new(
        region.x + region.width / 2.0,
        region.y + region.height / 2.0,
    );
    hover_at_point(center, Some(app_name), smooth)
}

/// Type text into an element ref (set value or keyboard fallback).
pub fn type_ref(
    r#ref: crate::core::element_tree::ElementRef,
    text: &str,
    app_name: &str,
) -> Result<ActionResult, ForepawError> {
    let _ = activate_app(app_name)?;
    let element = snapshot::resolve_ref_element(r#ref.id, app_name)?;
    set_value_on_element(element, text)
}

/// Type text via keyboard into whatever is focused.
pub fn keyboard_type(text: &str, app_name: Option<&str>) -> Result<ActionResult, ForepawError> {
    if let Some(app_name) = app_name {
        let _ = activate_app(app_name)?;
    }
    type_via_keyboard(text)?;
    Ok(ActionResult::ok_msg(format!("typed {} chars", text.len())))
}

/// Press a key combo, optionally activating an app first.
pub fn press_key(keys: &KeyCombo, app_name: Option<&str>) -> Result<ActionResult, ForepawError> {
    if let Some(app_name) = app_name {
        let _ = activate_app(app_name)?;
    }
    press_via_keyboard(keys)?;
    Ok(ActionResult::ok())
}

/// Scroll within an app's window.
pub fn scroll(
    direction: &str,
    amount: u32,
    app_name: &str,
    window: Option<&str>,
    r#ref: Option<crate::core::element_tree::ElementRef>,
    at: Option<Point>,
) -> Result<ActionResult, ForepawError> {
    let (_, pid, resolved) = activate_and_resolve_window(app_name, window)?;

    let scroll_point = if let Some(point) = at {
        // Scroll at window-relative coordinates
        let window_size = Point::new(resolved.bounds.width, resolved.bounds.height);
        crate::core::coordinate_validation::validate(&point, &window_size)
            .map_or(Ok(()), |e| Err(ForepawError::ActionFailed(e)))?;
        let screen = app::to_screen_point(&point, pid)?;
        CGPointFFI {
            x: screen.x,
            y: screen.y,
        }
    } else if let Some(r#ref) = r#ref {
        // Scroll at element center
        let element = snapshot::resolve_ref_element(r#ref.id, app_name)?;
        let pos = snapshot::get_element_position(element).ok_or_else(|| {
            ForepawError::ActionFailed(format!("Cannot determine position of {ref}"))
        })?;
        let (w, h) = snapshot::get_element_size(element)
            .ok_or_else(|| ForepawError::ActionFailed(format!("Cannot determine size of {ref}")))?;
        CGPointFFI {
            x: pos.x + w / 2.0,
            y: pos.y + h / 2.0,
        }
    } else {
        // Default: center of window
        let center = resolved.center();
        CGPointFFI {
            x: center.x,
            y: center.y,
        }
    };

    let (delta_y, delta_x) = match direction {
        "up" => (amount as i32, 0),
        "down" => (-(amount as i32), 0),
        "left" => (0, amount as i32),
        "right" => (0, -(amount as i32)),
        _ => {
            return Err(ForepawError::ActionFailed(format!(
                "Unknown direction '{direction}'. Use up, down, left, or right."
            )))
        }
    };

    // Move mouse to scroll target, wait for hover effects to settle
    move_mouse_to_scroll_target(scroll_point);
    thread::sleep(Duration::from_millis(150));

    // Capture fingerprint before scrolling
    let before_fingerprint = capture_scroll_fingerprint(resolved.window_id);

    post_scroll_event(delta_y, delta_x)?;
    thread::sleep(Duration::from_millis(150));

    // Detect boundary: compare content before and after
    let at_boundary = if let Some(before) = before_fingerprint {
        if let Some(after) = capture_scroll_fingerprint(resolved.window_id) {
            before == after
        } else {
            false
        }
    } else {
        false
    };

    let window_origin = resolved.origin();
    let rel_x = (scroll_point.x - window_origin.x) as i32;
    let rel_y = (scroll_point.y - window_origin.y) as i32;
    let boundary_note = if at_boundary {
        " (at boundary -- content did not change)"
    } else {
        ""
    };
    Ok(ActionResult::ok_msg(format!(
        "scrolled {direction} {amount} ticks at {rel_x},{rel_y}{boundary_note}"
    )))
}

/// Drag along a path of points.
pub fn drag_path(
    path: &[Point],
    options: &DragOptions,
    app_name: Option<&str>,
) -> Result<ActionResult, ForepawError> {
    if path.len() < 2 {
        return Err(ForepawError::ActionFailed(
            "Drag path requires at least 2 points".into(),
        ));
    }

    let cg_path: Vec<CGPointFFI> = if let Some(app_name) = app_name {
        let (_, pid) = activate_app(app_name)?;
        path.iter()
            .map(|p| app::to_screen_point(p, pid).map(|sp| CGPointFFI { x: sp.x, y: sp.y }))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        path.iter().map(|p| CGPointFFI { x: p.x, y: p.y }).collect()
    };

    let mut path = cg_path;
    if options.close_path && path.len() >= 3 {
        if let Some(first) = path.first().copied() {
            path.push(first);
        }
    }

    perform_mouse_drag(&path, options)?;

    // Report using the original input coordinates
    let msg = if path.len() == 2 {
        format!(
            "dragged from {},{} to {},{} ({} steps, {:.1}s)",
            path[0].x as i32,
            path[0].y as i32,
            path[1].x as i32,
            path[1].y as i32,
            options.steps,
            options.duration,
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

/// Drag from one element to another.
pub fn drag_refs(
    from_ref: crate::core::element_tree::ElementRef,
    to_ref: crate::core::element_tree::ElementRef,
    app_name: &str,
    options: &DragOptions,
) -> Result<ActionResult, ForepawError> {
    let (_, pid) = activate_app(app_name)?;

    let from = snapshot::resolve_ref_position(from_ref.id, app_name)?;
    let to = snapshot::resolve_ref_position(to_ref.id, app_name)?;

    let from_screen = app::to_screen_point(&from, pid)?;
    let to_screen = app::to_screen_point(&to, pid)?;

    let path = [
        CGPointFFI {
            x: from_screen.x,
            y: from_screen.y,
        },
        CGPointFFI {
            x: to_screen.x,
            y: to_screen.y,
        },
    ];
    perform_mouse_drag(&path, options)?;

    Ok(ActionResult::ok_msg(format!(
        "dragged from {},{} to {},{} ({} steps, {:.1}s)",
        from.x as i32, from.y as i32, to.x as i32, to.y as i32, options.steps, options.duration,
    )))
}
