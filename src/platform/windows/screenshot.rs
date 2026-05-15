//! Screenshot capture via Win32 GDI.
//!
//! Uses BitBlt for screen capture with per-monitor DPI awareness.
//! Physical pixels everywhere -- consistent with UIA bounding rectangles
//! and SendInput coordinates.

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

use crate::core::errors::ForepawError;
use crate::platform::windows::app;

/// Initialize DPI awareness for the process.
///
/// Must be called before any GDI capture calls. Per-monitor V2 gives us
/// physical pixel coordinates from all APIs (GetWindowRect, BitBlt, UIA).
pub fn init_dpi_awareness() {
    unsafe {
        // PER_MONITOR_AWARE_V2 = -4
        let ctx = DPI_AWARENESS_CONTEXT(-4_isize as *mut _);
        let _ = SetProcessDpiAwarenessContext(ctx);
    }
}

/// Generate a unique temp file tag.
///
/// Capture a screenshot of a specific window or the full screen.
///
/// Returns the path to the saved PNG file.
pub fn screenshot(app_name: Option<&str>, window: Option<&str>) -> Result<String, ForepawError> {
    let (rgba_pixels, width, height) = capture_pixels(app_name, window)?;
    save_pixels_to_temp(&rgba_pixels, width, height)
}

/// Capture screen/window pixels as RGBA.
///
/// Returns (rgba_bytes, width, height). Used by both screenshot and OCR.
///
/// For per-app capture: tries `PrintWindow` with `PW_RENDERFULLCONTENT` first.
/// This captures the window's own content directly, even when occluded by other
/// windows, and works for DWM-composed windows (UWP, Chromium, WinUI 3).
/// Falls back to desktop DC capture if PrintWindow fails.
///
/// For full-screen capture: uses desktop DC + BitBlt.
pub fn capture_pixels(
    app_name: Option<&str>,
    _window: Option<&str>,
) -> Result<(Vec<u8>, u32, u32), ForepawError> {
    if let Some(name) = app_name {
        let (hwnd, _) = app::find_app_hwnd(name)?;
        let rect = unsafe {
            let mut r = RECT::default();
            GetWindowRect(hwnd, &mut r)
                .map_err(|e| ForepawError::ActionFailed(format!("GetWindowRect failed: {e}")))?;
            r
        };

        let width = (rect.right - rect.left).max(1) as u32;
        let height = (rect.bottom - rect.top).max(1) as u32;

        // Try PrintWindow first -- captures window content even when occluded
        if let Ok(rgba) = capture_print_window(hwnd, width, height) {
            return Ok((rgba, width, height));
        }

        // Fall back to desktop DC capture
        let hdc = unsafe { GetDC(None) };
        let rgba = capture_region_rgba(hdc, rect.left, rect.top, width, height)?;
        unsafe { ReleaseDC(None, hdc) };
        Ok((rgba, width, height))
    } else {
        let rect = capture_rect_fullscreen();
        let width = (rect.right - rect.left).max(1) as u32;
        let height = (rect.bottom - rect.top).max(1) as u32;
        let hdc = unsafe { GetDC(None) };
        let rgba = capture_region_rgba(hdc, rect.left, rect.top, width, height)?;
        unsafe { ReleaseDC(None, hdc) };
        Ok((rgba, width, height))
    }
}

/// Capture a window via PrintWindow.
///
/// Uses `PW_RENDERFULLCONTENT` (undocumented flag, value 2) which allows
/// capturing windows rendered via DirectComposition (Chromium, UWP, etc.).
/// Chromium itself uses this flag internally.
fn capture_print_window(
    hwnd: windows::Win32::Foundation::HWND,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ()> {
    unsafe {
        let hdc = GetDC(None);
        let hdc_mem = CreateCompatibleDC(Some(hdc));
        let h_bitmap = CreateCompatibleBitmap(hdc, width as i32, height as i32);
        let old_bitmap = SelectObject(hdc_mem, HGDIOBJ::from(h_bitmap));

        // PW_RENDERFULLCONTENT = 2 -- captures DirectComposition-rendered content
        let flags = PRINT_WINDOW_FLAGS(2);
        let ok = PrintWindow(hwnd, hdc_mem, flags);

        if !ok.as_bool() {
            // PrintWindow failed -- clean up and let caller fall back
            SelectObject(hdc_mem, old_bitmap);
            let _ = DeleteObject(HGDIOBJ::from(h_bitmap));
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(None, hdc);
            return Err(());
        }

        let mut pixels = vec![0_u8; (width * height * 4) as usize];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // negative = top-down
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
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, old_bitmap);
        let _ = DeleteObject(HGDIOBJ::from(h_bitmap));
        let _ = DeleteDC(hdc_mem);
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

/// Save RGBA pixels to a temp PNG file. Returns the file path.
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
    let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

/// Capture a screen region as RGBA pixels using GDI BitBlt.
///
/// Captures at physical pixel resolution (requires DPI awareness to be set).
fn capture_region_rgba(
    hdc_source: HDC,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ForepawError> {
    unsafe {
        let hdc_mem = CreateCompatibleDC(Some(hdc_source));
        let h_bitmap = CreateCompatibleBitmap(hdc_source, width as i32, height as i32);
        let old_bitmap = SelectObject(hdc_mem, HGDIOBJ::from(h_bitmap));

        BitBlt(
            hdc_mem,
            0,
            0,
            width as i32,
            height as i32,
            Some(hdc_source),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        )
        .map_err(|e| ForepawError::ActionFailed(format!("BitBlt failed: {e}")))?;

        let mut pixels = vec![0_u8; (width * height * 4) as usize];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // negative = top-down
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
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // Cleanup GDI objects
        SelectObject(hdc_mem, old_bitmap);
        let _ = DeleteObject(HGDIOBJ::from(h_bitmap));
        let _ = DeleteDC(hdc_mem);

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
        let _ = fs::create_dir_all(parent);
    }

    img.save(path)
        .map_err(|e| ForepawError::ActionFailed(format!("failed to save PNG to {path}: {e}")))?;

    Ok(())
}
