//! Physical display enumeration via CoreGraphics (+ `NSScreen` for extras).
//!
//! Provides [`DisplayInfo`] for each online display: logical bounds, backing
//! scale factor, localized name, and primary/builtin flags. All bounds are in
//! the global logical (point) coordinate space shared with window and element
//! bounds.
//!
//! CoreGraphics (the data source for everything except name/color/refresh/hdr)
//! is thread-safe, so this module does not require a main-thread caller. Those
//! extras come from `NSScreen`, which is main-thread-only; they are looked up
//! best-effort and left `None` when the caller is off the main thread.

use std::collections::HashMap;

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;

use crate::core::types::Rect;
use crate::platform::DisplayInfo;

/// Enumerate all online displays.
///
/// Thread-safe: reads id, bounds, scale, primary, and builtin from
/// CoreGraphics. The `NSScreen`-derived fields (name, `color_space`,
/// `refresh_rate_hz`, `is_hdr`) are best-effort — `None` when called off the main
/// thread.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] only if `CGGetOnlineDisplayList`
/// fails, which shouldn't happen in practice.
pub fn displays() -> Result<Vec<DisplayInfo>, crate::core::errors::ForepawError> {
    use crate::platform::darwin::ffi;

    let max_displays = 16_u32;
    let mut display_ids = [0u32; 16];
    let mut display_count = 0_u32;
    // SAFETY: CGGetOnlineDisplayList writes to caller-provided buffers.
    let result = unsafe {
        ffi::CGGetOnlineDisplayList(
            max_displays,
            display_ids.as_mut_ptr(),
            &raw mut display_count,
        )
    };
    if result != 0 {
        return Err(crate::core::errors::ForepawError::ActionFailed(format!(
            "CGGetOnlineDisplayList failed (code {result})"
        )));
    }
    let count = usize::try_from(display_count).unwrap_or(0);

    // Best-effort NSScreen extras; all None off the main thread.
    let extras = screen_extras();

    // SAFETY: CGMainDisplayID is a read-only system call.
    let main_id = unsafe { ffi::CGMainDisplayID() };

    let mut out = Vec::with_capacity(count);
    for &display_id in display_ids.iter().take(count) {
        // SAFETY: CGDisplayBounds returns a CGRectFFI for a valid display ID.
        let cg_rect = unsafe { ffi::CGDisplayBounds(display_id) };
        let logical_w = cg_rect.size.width;
        let scale_factor = display_scale_factor(display_id, logical_w);
        // SAFETY: CGDisplayIsBuiltin is a read-only system call.
        let is_builtin = Some(unsafe { ffi::CGDisplayIsBuiltin(display_id) } != 0);
        let extras = extras.get(&display_id).cloned().unwrap_or_default();
        out.push(DisplayInfo {
            id: display_id,
            name: extras.name,
            logical_bounds: Rect::new(
                cg_rect.origin.x,
                cg_rect.origin.y,
                cg_rect.size.width,
                cg_rect.size.height,
            ),
            scale_factor,
            is_primary: display_id == main_id,
            is_builtin,
            color_space: extras.color_space,
            refresh_rate_hz: extras.refresh_rate_hz,
            is_hdr: extras.is_hdr,
            is_hdr_active: extras.is_hdr_active,
        });
    }
    Ok(out)
}

/// Backing scale factor for a display: physical pixels / logical points.
///
/// Derived from the current display mode (`CGDisplayCopyDisplayMode`) rather
/// than `CGDisplayPixelsWide`, which returns the scaled (logical) width under
/// scaled-resolution modes — e.g. a Retina display reports 1800 from both APIs
/// at the default 2x mode, masking the true 2.0 scale. The mode API's pixel
/// width is the framebuffer width (3600), giving the correct 2.0 ratio.
/// Falls back to 1.0 if the mode is missing or the logical width is zero.
fn display_scale_factor(display_id: u32, logical_w: f64) -> f64 {
    use crate::platform::darwin::ffi;
    if logical_w <= 0.0 {
        return 1.0;
    }
    // SAFETY: CGDisplayCopyDisplayMode returns a mode ref for a valid display.
    let mode = unsafe { ffi::CGDisplayCopyDisplayMode(display_id) };
    if mode.is_null() {
        return 1.0;
    }
    // SAFETY: mode is valid and non-null; all three calls take the mode ref.
    let px_w = unsafe { ffi::CGDisplayModeGetPixelWidth(mode) };
    // SAFETY: release the ref we copied above.
    unsafe { ffi::CGDisplayModeRelease(mode) };
    #[expect(clippy::cast_precision_loss, reason = "pixel count fits in f64")]
    let px = px_w as f64;
    px / logical_w
}

/// `NSScreen`-derived fields that CoreGraphics doesn't expose.
///
/// `name`/`color_space` own strings; the rest are `Copy`. Callers cloning out
/// of the map should clone the whole struct (cheap: two short strings).
#[derive(Default, Clone)]
struct ScreenExtras {
    name: Option<String>,
    color_space: Option<String>,
    refresh_rate_hz: Option<f64>,
    is_hdr: Option<bool>,
    is_hdr_active: Option<bool>,
}

/// Build a `CGDirectDisplayID → ScreenExtras` map from `NSScreen`.
///
/// Returns an empty map when the caller is not on the main thread (`NSScreen`
/// is main-thread-only). Every field is a best-effort nicety; none are load
/// bearing.
fn screen_extras() -> HashMap<u32, ScreenExtras> {
    use objc2_foundation::{NSNumber, NSString};

    let mut map = HashMap::new();

    let Some(mtm) = MainThreadMarker::new() else {
        return map;
    };
    for screen in &NSScreen::screens(mtm) {
        let key = NSString::from_str("NSScreenNumber");
        let Some(obj) = screen.deviceDescription().objectForKey(&key) else {
            continue;
        };
        let Ok(number) = obj.downcast::<NSNumber>() else {
            continue;
        };
        let id = number.unsignedIntValue();

        // Color space: localized name like "Display P3", "sRGB".
        let color_space = screen
            .colorSpace()
            .and_then(|cs| cs.localizedName())
            .map(|s| s.to_string());

        // Refresh rate: AppKit exposes an integer FPS.
        #[expect(clippy::cast_precision_loss, reason = "FPS fits in f64")]
        let refresh_rate_hz = Some(screen.maximumFramesPerSecond() as f64);

        // HDR/EDR: two distinct signals.
        //   - `maximumPotentialExtendedDynamicRangeColorComponentValue` reports
        //     hardware capability. The non-potential variant reports the current
        //     EDR ceiling, which reads 1.0 whenever EDR isn't actively engaged
        //     -- so a capable-but-idle MacBook Pro panel would falsely report
        //     non-HDR. Potential reports 16.0 on a Liquid Retina XDR, 1.0 on an
        //     SDR panel.
        //   - The current ceiling distinguishes "HDR is engaged right now"
        //     (an app opted into EDR) from "the panel could do HDR."
        let is_hdr = Some(screen.maximumPotentialExtendedDynamicRangeColorComponentValue() > 1.0);
        let is_hdr_active = Some(screen.maximumExtendedDynamicRangeColorComponentValue() > 1.0);

        map.insert(
            id,
            ScreenExtras {
                name: Some(screen.localizedName().to_string()),
                color_space,
                refresh_rate_hz,
                is_hdr,
                is_hdr_active,
            },
        );
    }
    map
}
