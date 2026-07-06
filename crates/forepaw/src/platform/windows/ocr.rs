//! OCR via Windows.Media.Ocr (`WinRT`).
//!
//! Captures a screenshot via GDI, creates a `SoftwareBitmap` from the raw pixels,
//! and runs `OcrEngine` on it. Coordinates are physical pixels relative to the
//! captured window's top-left (the `GetWindowRect` origin), not screen-absolute.

use windows::Foundation::Rect as WinRect;
use windows::Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::WinRT::IMemoryBufferByteAccess;

use std::fmt::Write;

use crate::core::encoder_detection::ScreenshotOptions;
use crate::core::errors::ForepawError;
use crate::core::key_combo::{ClickOptions, MouseButton};
use crate::core::ocr_result::{OCROutput, OCRResult};
use crate::core::types::{Dimensions, Point, Rect};
use crate::platform::{ActionResult, AppTarget, WindowTarget};

/// Run OCR on an app window (or full screen).
///
/// Captures a screenshot, runs OCR on it, returns recognized text with
/// bounding boxes in physical pixel coordinates.
///
/// # Errors
///
/// Returns [`ForepawError::ScreenRecordingDenied`] if screen capture fails,
/// or [`ForepawError::AppNotFound`] if the target application is not found.
pub fn ocr(
    app: Option<&AppTarget>,
    window: Option<&WindowTarget>,
    find: Option<&str>,
    screenshot_options: Option<&ScreenshotOptions>,
) -> Result<OCROutput, ForepawError> {
    // Capture screenshot as raw RGBA pixels
    let (rgba_pixels, width, height) =
        crate::platform::windows::screenshot::capture_pixels(app, window)?;

    // Upscale 2x before OCR -- Windows.Media.Ocr struggles with small text.
    // Lanczos3 preserves sharpness better than bilinear for text edges.
    let scale = 3_u32;
    let (ocr_pixels, ocr_dims) = upscale_rgba(&rgba_pixels, width, height, scale);

    // Create SoftwareBitmap from the upscaled pixel data
    let bitmap = create_software_bitmap(&ocr_pixels, ocr_dims.width, ocr_dims.height)?;

    // Run OCR
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| ForepawError::ActionFailed(format!("OcrEngine creation failed: {e}")))?;

    let async_op = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| ForepawError::ActionFailed(format!("RecognizeAsync start failed: {e}")))?;

    let ocr_result = block_on_async(&async_op)?;

    // Extract results
    let lines = ocr_result
        .Lines()
        .map_err(|e| ForepawError::ActionFailed(format!("Lines() failed: {e}")))?;

    let mut results = Vec::new();
    let line_count = lines.Size().unwrap_or(0);
    for i in 0..line_count {
        let line = lines
            .GetAt(i)
            .map_err(|e| ForepawError::ActionFailed(format!("GetAt({i}) failed: {e}")))?;

        let text = line
            .Text()
            .map_err(|e| ForepawError::ActionFailed(format!("Text() failed: {e}")))?
            .to_string();

        let words = line
            .Words()
            .map_err(|e| ForepawError::ActionFailed(format!("Words() failed: {e}")))?;

        let word_count = words.Size().unwrap_or(0);
        // Build the line-level bounding box from word positions
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for j in 0..word_count {
            let word = words
                .GetAt(j)
                .map_err(|e| ForepawError::ActionFailed(format!("word GetAt({j}) failed: {e}")))?;
            let wr: WinRect = word
                .BoundingRect()
                .map_err(|e| ForepawError::ActionFailed(format!("BoundingRect failed: {e}")))?;
            min_x = min_x.min(wr.X);
            min_y = min_y.min(wr.Y);
            max_x = max_x.max(wr.X + wr.Width);
            max_y = max_y.max(wr.Y + wr.Height);
        }

        // Fallback if no words
        if word_count == 0 {
            min_x = 0.0;
            min_y = 0.0;
            max_x = 0.0;
            max_y = 0.0;
        }

        results.push(OCRResult {
            text,
            bounds: Rect::new(
                f64::from(min_x) / f64::from(scale),
                f64::from(min_y) / f64::from(scale),
                f64::from(max_x - min_x) / f64::from(scale),
                f64::from(max_y - min_y) / f64::from(scale),
            ),
        });
    }

    // Filter if searching for specific text
    if let Some(query) = find {
        let q = query.to_lowercase();
        results.retain(|r| r.text.to_lowercase().contains(&q));
    }

    // Optionally save a display screenshot
    let display_path = if screenshot_options.is_some() {
        let path =
            crate::platform::windows::screenshot::save_pixels_to_temp(&rgba_pixels, width, height)?;
        Some(path)
    } else {
        None
    };

    Ok(OCROutput {
        results,
        screenshot_path: display_path,
    })
}

/// Resolve OCR text to a window-relative center point.
///
/// OCR coordinates are physical pixels relative to the window's top-left
/// (the capture origin from `GetWindowRect`), which is exactly the input
/// `to_screen_point` expects to translate to screen-absolute coordinates.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the text is not found,
/// or multiple matches exist without an explicit index.
fn resolve_ocr_text(
    text: &str,
    app: &AppTarget,
    window: Option<&WindowTarget>,
    index: Option<usize>,
) -> Result<(String, Point), ForepawError> {
    let output = ocr(Some(app), window, Some(text), None)?;
    let matches = output.results;

    if matches.is_empty() {
        return Err(ForepawError::ActionFailed(format!(
            "No text matching '{text}' found on screen"
        )));
    }

    if matches.len() > 1 && index.is_none() {
        let mut listing = format!("Multiple matches for '{text}':\n");
        for (i, m) in matches.iter().enumerate() {
            // write! to String is infallible, discard is intentional.
            #[expect(clippy::let_underscore_must_use)]
            let _ = writeln!(
                listing,
                "  --index {}: '{}' at {:.0},{:.0}",
                i + 1,
                m.text,
                m.center().0,
                m.center().1
            );
        }
        listing += "Use --index N to pick one.";
        return Err(ForepawError::ActionFailed(listing));
    }

    let resolved_index = index.unwrap_or(1).saturating_sub(1);
    if resolved_index >= matches.len() {
        return Err(ForepawError::ActionFailed(format!(
            "--index {} out of range ({} matches found)",
            index.unwrap_or(0),
            matches.len()
        )));
    }

    let Some(match_result) = matches.get(resolved_index) else {
        // Should be unreachable due to bounds check above, but handle defensively
        return Err(ForepawError::ActionFailed(format!(
            "OCR index {resolved_index} out of range ({} results)",
            matches.len()
        )));
    };
    Ok((
        match_result.text.clone(),
        Point::new(match_result.center().0, match_result.center().1),
    ))
}

/// OCR-click: find text on screen and click it.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the text is not found in OCR results,
/// or the underlying click fails.
pub fn ocr_click(
    text: &str,
    app: &AppTarget,
    window: Option<&WindowTarget>,
    options: &ClickOptions,
    index: Option<usize>,
) -> Result<ActionResult, ForepawError> {
    let (matched_text, window_point) = resolve_ocr_text(text, app, window, index)?;

    super::app::activate_app(app)?;
    let screen_point = super::app::to_screen_point(&window_point, app)?;
    super::input::perform_mouse_click(screen_point, options.button, options.click_count)?;

    let label = match (options.button, options.click_count) {
        (MouseButton::Right, _) => "right-clicked",
        (MouseButton::Left, n) if n > 1 => "double-clicked",
        (MouseButton::Left, _) => "clicked",
    };
    Ok(ActionResult::ok_msg(format!(
        "{label} '{matched_text}' at {:.0},{:.0}",
        window_point.x, window_point.y
    )))
}

/// OCR-hover: find text on screen and hover at its position.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the text is not found in OCR results,
/// or the underlying hover fails.
pub fn ocr_hover(
    text: &str,
    app: &AppTarget,
    window: Option<&WindowTarget>,
    index: Option<usize>,
) -> Result<ActionResult, ForepawError> {
    let (matched_text, window_point) = resolve_ocr_text(text, app, window, index)?;

    super::app::activate_app(app)?;
    let screen_point = super::app::to_screen_point(&window_point, app)?;
    super::input::hover_move(screen_point)?;

    Ok(ActionResult::ok_msg(format!(
        "hovered '{matched_text}' at {:.0},{:.0}",
        window_point.x, window_point.y
    )))
}

/// Wait for text to appear on screen via OCR polling.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the text is not found before the timeout,
/// or if screen capture fails.
pub fn wait(
    text: &str,
    app: &AppTarget,
    window: Option<&WindowTarget>,
    timeout: f64,
    interval: f64,
) -> Result<ActionResult, ForepawError> {
    let start = std::time::Instant::now();
    let timeout_dur = std::time::Duration::from_secs_f64(timeout);
    let interval_dur = std::time::Duration::from_secs_f64(interval);

    loop {
        match ocr(Some(app), window, Some(text), None) {
            Ok(output) if !output.results.is_empty() => {
                let Some(matched) = output.results.first() else {
                    continue;
                };
                return Ok(ActionResult::ok_msg(format!(
                    "found '{}' after waiting",
                    matched.text
                )));
            }
            _ => {}
        }

        if start.elapsed() >= timeout_dur {
            break;
        }
        std::thread::sleep(interval_dur);
    }

    Err(ForepawError::ActionFailed(format!(
        "Timed out after {timeout:.0}s waiting for '{text}'"
    )))
}

/// Create a `SoftwareBitmap` from RGBA pixel data.
///
/// The `OcrEngine` expects BGRA, so we convert RGBA -> BGRA during copy.
#[expect(
    clippy::indexing_slicing,
    reason = "pixel buffer RGBA->BGRA swap with loop-bounded indices"
)]
fn create_software_bitmap(
    rgba_pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<SoftwareBitmap, ForepawError> {
    #[expect(clippy::cast_possible_wrap, reason = "image dimensions fit in i32")]
    let w = width as i32;
    #[expect(clippy::cast_possible_wrap, reason = "image dimensions fit in i32")]
    let h = height as i32;
    let bitmap = SoftwareBitmap::Create(BitmapPixelFormat::Bgra8, w, h)
        .map_err(|e| ForepawError::ActionFailed(format!("SoftwareBitmap::Create failed: {e}")))?;

    // Lock the buffer for writing
    let buffer = bitmap
        .LockBuffer(BitmapBufferAccessMode::Write)
        .map_err(|e| ForepawError::ActionFailed(format!("LockBuffer failed: {e}")))?;

    let reference = buffer
        .CreateReference()
        .map_err(|e| ForepawError::ActionFailed(format!("CreateReference failed: {e}")))?;

    // Get raw pointer via IMemoryBufferByteAccess
    let byte_access: IMemoryBufferByteAccess =
        windows::core::Interface::cast(&reference).map_err(|e| {
            ForepawError::ActionFailed(format!("IMemoryBufferByteAccess cast failed: {e}"))
        })?;

    let mut ptr: *mut u8 = std::ptr::null_mut();
    let mut capacity: u32 = 0;
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        byte_access
            .GetBuffer(&raw mut ptr, &raw mut capacity)
            .map_err(|e| ForepawError::ActionFailed(format!("GetBuffer failed: {e}")))?;
    }

    // Copy RGBA -> BGRA
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let dst = unsafe { std::slice::from_raw_parts_mut(ptr, capacity as usize) };
    let chunk_count = std::cmp::min(rgba_pixels.len(), dst.len()) / 4;
    for i in 0..chunk_count {
        let si = i * 4;
        dst[si] = rgba_pixels[si + 2]; // B <- R
        dst[si + 1] = rgba_pixels[si + 1]; // G
        dst[si + 2] = rgba_pixels[si]; // R <- B
        dst[si + 3] = rgba_pixels[si + 3]; // A
    }

    // Buffer is released when it goes out of scope
    drop(reference);
    drop(buffer);

    Ok(bitmap)
}

/// Block on a `WinRT` async operation using a Win32 event.
///
/// Sets up a Completed callback that signals a Win32 event when the
/// operation finishes, then waits and calls `GetResults`.
fn block_on_async<T: windows::core::RuntimeType + 'static>(
    op: &windows_future::IAsyncOperation<T>,
) -> Result<T, ForepawError> {
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let event = unsafe {
        CreateEventW(None, true, false, windows::core::PCWSTR::null())
            .map_err(|e| ForepawError::ActionFailed(format!("CreateEventW failed: {e}")))?
    };

    // HANDLE is not Send, extract the raw value for the closure
    let event_raw: isize = event.0 as isize;

    let handler = windows_future::AsyncOperationCompletedHandler::new(move |_info, _status| {
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        unsafe {
            SetEvent(windows::Win32::Foundation::HANDLE(event_raw as *mut _))
                .ok()
                .unwrap_or_default();
        }
        Ok(())
    });

    op.SetCompleted(&handler)
        .map_err(|e| ForepawError::ActionFailed(format!("SetCompleted failed: {e}")))?;

    // Wait for completion (10 second timeout)
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "Win32/WinRT FFI pipeline"
    )]
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        let _wait: u32 = WaitForSingleObject(event, 10000).0;
        CloseHandle(event).ok().unwrap_or_default();
    }

    op.GetResults()
        .map_err(|e| ForepawError::ActionFailed(format!("OCR GetResults failed: {e}")))
}

/// Upscale RGBA pixels by an integer factor using Lanczos3 resampling.
///
/// Returns (`upscaled_rgba`, `new_width`, `new_height`).
fn upscale_rgba(rgba: &[u8], width: u32, height: u32, scale: u32) -> (Vec<u8>, Dimensions) {
    let new_w = width * scale;
    let new_h = height * scale;
    crate::platform::windows::image_ops::resize_rgba(
        rgba,
        Dimensions::new(width, height),
        Dimensions::new(new_w, new_h),
    )
    .unwrap_or_else(|| (rgba.to_vec(), Dimensions::new(width, height)))
}
