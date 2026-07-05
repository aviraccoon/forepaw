//! Screenshot capture via Win32 GDI.
//!
//! Uses `BitBlt` for screen capture with per-monitor DPI awareness.
//! Physical pixels everywhere -- consistent with UIA bounding rectangles
//! and `SendInput` coordinates.

use std::fs;
use std::path::Path;

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HDC,
    HGDIOBJ, SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

use crate::core::display::display_for_bounds;
use crate::core::encoder_detection::CaptureScale;
use crate::core::errors::ForepawError;
use crate::core::types::{Dimensions, Rect};
use crate::platform::windows::app;
use crate::platform::{AppTarget, WindowTarget};

/// Initialize DPI awareness for the process.
///
/// Must be called before any GDI capture calls. Per-monitor V2 gives us
/// physical pixel coordinates from all APIs (`GetWindowRect`, `BitBlt`, UIA).
pub fn init_dpi_awareness() {
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        // PER_MONITOR_AWARE_V2 = -4
        let ctx = DPI_AWARENESS_CONTEXT(-4_isize as *mut _);
        SetProcessDpiAwarenessContext(ctx).ok().unwrap_or_default();
    }
}

/// Generate a unique temp file tag.
///
/// Result of a Windows capture: the saved PNG path plus the metadata a
/// [`ScreenshotResult`](crate::platform::ScreenshotResult) needs to report.
///
/// `pixels_per_bound_unit` is pixels per bound-unit of the saved image (1.0 for Native,
/// 1/display-scale for Logical); `dimensions` is its actual pixel size.
#[derive(Debug)]
pub struct CapturedImage {
    /// Saved temp PNG path.
    pub path: String,
    /// Pixels per bound-unit of the saved image.
    pub pixels_per_bound_unit: f64,
    /// Actual pixel width/height of the saved image.
    pub dimensions: Dimensions,
}

/// Capture a screenshot of a specific window or the full screen, optionally
/// downsampled to logical resolution.
///
/// `scale` controls the returned image: [`CaptureScale::Native`] returns the
/// physical capture unchanged; [`CaptureScale::Logical`] downsamples by the
/// display's scale factor (Windows runs `PER_MACHINE_AWARE_V2`, so window
/// bounds are physical pixels; logical = physical / display-scale).
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the target application is not found,
/// or [`ForepawError::ActionFailed`] if screen capture or file save fails.
pub fn screenshot(
    app: Option<&AppTarget>,
    window: Option<&WindowTarget>,
    scale: CaptureScale,
) -> Result<CapturedImage, ForepawError> {
    // Physical capture first; Logical downsamples from it below.
    let (rgba_pixels, phys_w, phys_h) = capture_pixels(app, window)?;

    // Native = 1.0 (image matches physical-pixel bounds). Logical = 1/scale
    // (image is smaller than the bounds); the consumer's (bounds-origin)*sf
    // still lands in the downsampled pixel space.
    let source_scale = window_display_scale(app, window);
    let scale_factor = match scale {
        CaptureScale::Native => 1.0,
        CaptureScale::Logical => 1.0 / source_scale,
    };

    if scale == CaptureScale::Logical && source_scale > 1.0 {
        let (path, dims) = downsample(&rgba_pixels, phys_w, phys_h, source_scale)?;
        Ok(CapturedImage {
            path,
            pixels_per_bound_unit: scale_factor,
            dimensions: dims,
        })
    } else {
        let path = save_pixels_to_temp(&rgba_pixels, phys_w, phys_h)?;
        Ok(CapturedImage {
            path,
            pixels_per_bound_unit: scale_factor,
            dimensions: Dimensions::new(phys_w, phys_h),
        })
    }
}

/// Backing scale of the display the captured window sits on (1.0 for
/// full-screen, where the main display's scale is the best single value for a
/// potentially multi-display composite capture).
fn window_display_scale(app: Option<&AppTarget>, window: Option<&WindowTarget>) -> f64 {
    let bounds = match app {
        Some(a) => match app::find_app_hwnd(a, window) {
            Ok((_, rect)) => Some(rect),
            // Capture already succeeded above; if the HWND lookup fails here
            // (shouldn't), fall back to main scale rather than erroring.
            Err(_) => None,
        },
        None => None,
    };
    let Some(bounds) = bounds else {
        return main_display_scale();
    };
    match crate::platform::windows::display::displays() {
        Ok(ds) => display_for_bounds(&ds, bounds).map_or(main_display_scale(), |d| d.scale_factor),
        Err(_) => main_display_scale(),
    }
}

/// Main display's backing scale, via `displays()` (1.0 if enumeration fails).
fn main_display_scale() -> f64 {
    crate::platform::windows::display::displays()
        .ok()
        .and_then(|ds| ds.iter().find(|d| d.is_primary).map(|d| d.scale_factor))
        .unwrap_or(1.0)
}

/// Downsample RGBA pixels by `factor` (physical → logical) and save as PNG.
/// Returns the saved path and the logical dimensions.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the resize or PNG save fails.
fn downsample(
    rgba: &[u8],
    phys_w: u32,
    phys_h: u32,
    factor: f64,
) -> Result<(String, Dimensions), ForepawError> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "dims fit in u32 and are non-negative"
    )]
    let new_w = ((f64::from(phys_w) / factor).round() as u32).max(1);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "dims fit in u32 and are non-negative"
    )]
    let new_h = ((f64::from(phys_h) / factor).round() as u32).max(1);
    let (resized, dims) = super::image_ops::resize_rgba(
        rgba,
        Dimensions::new(phys_w, phys_h),
        Dimensions::new(new_w, new_h),
    )
    .ok_or_else(|| ForepawError::ActionFailed("failed to downsample capture".into()))?;
    let path = save_pixels_to_temp(&resized, dims.width, dims.height)?;
    Ok((path, dims))
}

/// Capture screen/window pixels as RGBA.
///
/// Returns (`rgba_bytes`, width, height). Used by both screenshot and OCR.
///
/// For per-app capture: tries `PrintWindow` with `PW_RENDERFULLCONTENT` first.
/// This captures the window's own content directly, even when occluded by other
/// windows, and works for DWM-composed windows (UWP, Chromium, `WinUI` 3).
/// Falls back to desktop DC capture if `PrintWindow` fails.
///
/// For full-screen capture: uses desktop DC + `BitBlt`.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the target application is not found,
/// or [`ForepawError::ActionFailed`] if both `PrintWindow` and desktop DC capture fail.
pub fn capture_pixels(
    app: Option<&AppTarget>,
    window: Option<&WindowTarget>,
) -> Result<(Vec<u8>, u32, u32), ForepawError> {
    if let Some(name) = app {
        let (hwnd, _) = app::find_app_hwnd(name, window)?;
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        let rect = unsafe {
            let mut r = RECT::default();
            GetWindowRect(hwnd, &raw mut r)
                .map_err(|e| ForepawError::ActionFailed(format!("GetWindowRect failed: {e}")))?;
            r
        };

        #[expect(clippy::cast_sign_loss, reason = "max(1) ensures non-negative")]
        let width = (rect.right - rect.left).max(1) as u32;
        #[expect(clippy::cast_sign_loss, reason = "max(1) ensures non-negative")]
        let height = (rect.bottom - rect.top).max(1) as u32;

        // Try PrintWindow first -- captures window content even when occluded
        if let Ok(rgba) = capture_print_window(hwnd, width, height) {
            return Ok((rgba, width, height));
        }

        // Fall back to desktop DC capture
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        let hdc = unsafe { GetDC(None) };
        let rgba = capture_region_rgba(hdc, rect.left, rect.top, width, height)?;
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        unsafe { ReleaseDC(None, hdc) };
        Ok((rgba, width, height))
    } else {
        let rect = capture_rect_fullscreen();
        #[expect(clippy::cast_sign_loss, reason = "max(1) ensures non-negative")]
        let width = (rect.right - rect.left).max(1) as u32;
        #[expect(clippy::cast_sign_loss, reason = "max(1) ensures non-negative")]
        let height = (rect.bottom - rect.top).max(1) as u32;
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        let hdc = unsafe { GetDC(None) };
        let rgba = capture_region_rgba(hdc, rect.left, rect.top, width, height)?;
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        unsafe { ReleaseDC(None, hdc) };
        Ok((rgba, width, height))
    }
}

/// Capture a window via `PrintWindow`.
///
/// Uses `PW_RENDERFULLCONTENT` (undocumented flag, value 2) which allows
/// capturing windows rendered via `DirectComposition` (Chromium, UWP, etc.).
/// Chromium itself uses this flag internally.
fn capture_print_window(
    hwnd: windows::Win32::Foundation::HWND,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ()> {
    #[expect(clippy::cast_possible_wrap, reason = "image dimensions fit in i32")]
    let w = width as i32;
    #[expect(clippy::cast_possible_wrap, reason = "image dimensions fit in i32")]
    let h = height as i32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "BITMAPINFOHEADER is a fixed Win32 struct (~40 bytes)"
    )]
    let bi_size = size_of::<BITMAPINFOHEADER>() as u32;

    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "Win32/WinRT FFI pipeline"
    )]
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        let hdc = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc));
        let h_bitmap = CreateCompatibleBitmap(hdc, w, h);
        let old_bitmap = SelectObject(hdc_mem, HGDIOBJ::from(h_bitmap));

        // PW_RENDERFULLCONTENT = 2 -- captures DirectComposition-rendered content
        let flags = PRINT_WINDOW_FLAGS(2);
        let ok = PrintWindow(hwnd, hdc_mem, flags);

        if !ok.as_bool() {
            // PrintWindow failed -- clean up and let caller fall back
            SelectObject(hdc_mem, old_bitmap);
            DeleteObject(HGDIOBJ::from(h_bitmap))
                .ok()
                .unwrap_or_default();
            DeleteDC(hdc_mem).ok().unwrap_or_default();
            ReleaseDC(None, hdc);
            return Err(());
        }

        let mut pixels = vec![0_u8; (width * height * 4) as usize];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: bi_size,
                biWidth: w,
                biHeight: -h, // negative = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let result = GetDIBits(
            hdc_mem,
            h_bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &raw mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old_bitmap);
        DeleteObject(HGDIOBJ::from(h_bitmap))
            .ok()
            .unwrap_or_default();
        DeleteDC(hdc_mem).ok().unwrap_or_default();
        ReleaseDC(None, hdc);

        if result == 0 {
            return Err(());
        }

        // Convert BGRA -> RGBA
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }

        Ok(pixels)
    }
}

/// Capture a pixel fingerprint of a horizontal strip from the vertical center
/// of `bounds` (screen-absolute physical px), excluding the rightmost 30px to
/// avoid scrollbar overlays. Used by `scroll` for boundary detection: equal
/// fingerprints before/after mean the content did not change. Captures from the
/// screen DC, so the window must be foreground (callers activate first).
#[must_use]
pub(super) fn capture_strip_fingerprint(bounds: Rect) -> Option<Vec<u8>> {
    #[expect(clippy::cast_possible_truncation, reason = "screen coords fit in i32")]
    let strip_x = bounds.x.round() as i32;
    #[expect(clippy::cast_possible_truncation, reason = "screen coords fit in i32")]
    let strip_y = (bounds.y + bounds.height / 2.0 - 10.0).round() as i32;
    #[expect(clippy::cast_possible_truncation, reason = "screen width fits in i32")]
    let strip_w = (bounds.width.round() as i32 - 30).max(1);
    let strip_h = 20_i32;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "BITMAPINFOHEADER is a fixed Win32 struct (~40 bytes)"
    )]
    let bi_size = size_of::<BITMAPINFOHEADER>() as u32;

    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "GDI capture pipeline")]
    // SAFETY: GDI strip capture from the screen DC; all handles released/deleted.
    unsafe {
        let hdc = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc));
        let h_bitmap = CreateCompatibleBitmap(hdc, strip_w, strip_h);
        let old_bitmap = SelectObject(hdc_mem, HGDIOBJ::from(h_bitmap));

        let blit_ok = BitBlt(
            hdc_mem,
            0,
            0,
            strip_w,
            strip_h,
            Some(hdc),
            strip_x,
            strip_y,
            SRCCOPY,
        );

        let w = strip_w.unsigned_abs();
        let h = strip_h.unsigned_abs();
        let mut pixels = vec![0_u8; (w as usize) * (h as usize) * 4];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: bi_size,
                biWidth: strip_w,
                biHeight: -strip_h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let got = GetDIBits(
            hdc_mem,
            h_bitmap,
            0,
            h,
            Some(pixels.as_mut_ptr().cast()),
            &raw mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old_bitmap);
        DeleteObject(HGDIOBJ::from(h_bitmap))
            .ok()
            .unwrap_or_default();
        DeleteDC(hdc_mem).ok().unwrap_or_default();
        ReleaseDC(None, hdc);

        if blit_ok.is_err() || got == 0 {
            None
        } else {
            Some(pixels)
        }
    }
}

/// Save RGBA pixels to a temp PNG file. Returns the file path.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if PNG encoding or file writing fails.
pub fn save_pixels_to_temp(
    rgba_pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<String, ForepawError> {
    let tag = crate::core::temp::temp_tag();
    let path = format!(
        "{}\\forepaw-{tag}.png",
        std::env::temp_dir().to_string_lossy()
    );
    save_png(rgba_pixels, width, height, &path)?;
    Ok(path)
}

/// Get the capture rectangle for the full virtual screen.
fn capture_rect_fullscreen() -> RECT {
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

/// Capture a screen region as RGBA pixels using GDI `BitBlt`.
///
/// Captures at physical pixel resolution (requires DPI awareness to be set).
fn capture_region_rgba(
    hdc_source: HDC,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ForepawError> {
    #[expect(clippy::cast_possible_wrap, reason = "image dimensions fit in i32")]
    let w = width as i32;
    #[expect(clippy::cast_possible_wrap, reason = "image dimensions fit in i32")]
    let h = height as i32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "BITMAPINFOHEADER is a fixed Win32 struct (~40 bytes)"
    )]
    let bi_size = size_of::<BITMAPINFOHEADER>() as u32;

    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "Win32/WinRT FFI pipeline"
    )]
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        let hdc_mem = CreateCompatibleDC(Some(hdc_source));
        let h_bitmap = CreateCompatibleBitmap(hdc_source, w, h);
        let old_bitmap = SelectObject(hdc_mem, HGDIOBJ::from(h_bitmap));

        BitBlt(
            hdc_mem,
            0,
            0,
            w,
            h,
            Some(hdc_source),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        )
        .map_err(|e| ForepawError::ActionFailed(format!("BitBlt failed: {e}")))?;

        let mut pixels = vec![0_u8; (width * height * 4) as usize];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: bi_size,
                biWidth: w,
                biHeight: -h, // negative = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let result = GetDIBits(
            hdc_mem,
            h_bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &raw mut bmi,
            DIB_RGB_COLORS,
        );

        // Cleanup GDI objects
        SelectObject(hdc_mem, old_bitmap);
        DeleteObject(HGDIOBJ::from(h_bitmap))
            .ok()
            .unwrap_or_default();
        DeleteDC(hdc_mem).ok().unwrap_or_default();

        if result == 0 {
            return Err(ForepawError::ActionFailed("GetDIBits failed".into()));
        }

        // Convert BGRA -> RGBA
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }

        Ok(pixels)
    }
}

/// Save raw RGBA pixels as a PNG file.
fn save_png(rgba_pixels: &[u8], width: u32, height: u32, path: &str) -> Result<(), ForepawError> {
    let img = image::RgbaImage::from_raw(width, height, rgba_pixels.to_vec()).ok_or_else(|| {
        ForepawError::ActionFailed("failed to create image from pixel data".into())
    })?;

    // Create parent directory if needed
    if let Some(parent) = Path::new(path).parent() {
        drop(fs::create_dir_all(parent));
    }

    img.save(path)
        .map_err(|e| ForepawError::ActionFailed(format!("failed to save PNG to {path}: {e}")))?;

    Ok(())
}
