//! OCR via Windows.Media.Ocr (WinRT).
//!
//! Captures a screenshot via GDI, creates a SoftwareBitmap from the raw pixels,
//! and runs OcrEngine on it. All coordinates are in physical pixels, matching
//! UIA bounding rectangles.

use windows::Foundation::Rect as WinRect;
use windows::Graphics::Imaging::{
    BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap,
};
use windows::Media::Ocr::OcrEngine;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::WinRT::IMemoryBufferByteAccess;

use crate::core::errors::ForepawError;
use crate::core::ocr_result::{OCROutput, OCRResult};
use crate::core::types::Rect;
use crate::platform::ScreenshotOptions;

/// Run OCR on an app window (or full screen).
///
/// Captures a screenshot, runs OCR on it, returns recognized text with
/// bounding boxes in physical pixel coordinates.
pub fn ocr(
    app_name: Option<&str>,
    window: Option<&str>,
    find: Option<&str>,
    screenshot_options: Option<&ScreenshotOptions>,
) -> Result<OCROutput, ForepawError> {
    // Capture screenshot as raw RGBA pixels
    let (rgba_pixels, width, height) =
        crate::platform::windows::screenshot::capture_pixels(app_name, window)?;

    // Upscale 2x before OCR -- Windows.Media.Ocr struggles with small text.
    // Lanczos3 preserves sharpness better than bilinear for text edges.
    let scale = 3u32;
    let (ocr_pixels, ocr_width, ocr_height) = upscale_rgba(&rgba_pixels, width, height, scale);

    // Create SoftwareBitmap from the upscaled pixel data
    let bitmap = create_software_bitmap(&ocr_pixels, ocr_width, ocr_height)?;

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
            let word = words.GetAt(j).map_err(|e| {
                ForepawError::ActionFailed(format!("word GetAt({j}) failed: {e}"))
            })?;
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
                (min_x / scale as f32) as f64,
                (min_y / scale as f32) as f64,
                ((max_x - min_x) / scale as f32) as f64,
                ((max_y - min_y) / scale as f32) as f64,
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
        let path = crate::platform::windows::screenshot::save_pixels_to_temp(
            &rgba_pixels, width, height,
        )?;
        Some(path)
    } else {
        None
    };

    Ok(OCROutput {
        results,
        screenshot_path: display_path,
    })
}

/// Create a SoftwareBitmap from RGBA pixel data.
///
/// The OcrEngine expects BGRA, so we convert RGBA -> BGRA during copy.
fn create_software_bitmap(
    rgba_pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<SoftwareBitmap, ForepawError> {
    let bitmap = SoftwareBitmap::Create(BitmapPixelFormat::Bgra8, width as i32, height as i32)
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
    unsafe {
        byte_access
            .GetBuffer(&mut ptr, &mut capacity)
            .map_err(|e| ForepawError::ActionFailed(format!("GetBuffer failed: {e}")))?;
    }

    // Copy RGBA -> BGRA
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

/// Block on a WinRT async operation using a Win32 event.
///
/// Sets up a Completed callback that signals a Win32 event when the
/// operation finishes, then waits and calls GetResults.
fn block_on_async<T: windows::core::RuntimeType + 'static>(
    op: &windows_future::IAsyncOperation<T>,
) -> Result<T, ForepawError> {
    let event = unsafe {
        CreateEventW(None, true, false, windows::core::PCWSTR::null())
            .map_err(|e| ForepawError::ActionFailed(format!("CreateEventW failed: {e}")))?
    };

    // HANDLE is not Send, extract the raw value for the closure
    let event_raw: isize = event.0 as isize;

    let handler = windows_future::AsyncOperationCompletedHandler::new(move |_info, _status| {
        unsafe {
            let _ = SetEvent(windows::Win32::Foundation::HANDLE(event_raw as *mut _));
        }
        Ok(())
    });

    op.SetCompleted(&handler)
        .map_err(|e| ForepawError::ActionFailed(format!("SetCompleted failed: {e}")))?;

    // Wait for completion (10 second timeout)
    unsafe {
        let _ = WaitForSingleObject(event, 10000);
        let _ = CloseHandle(event);
    }

    op.GetResults()
        .map_err(|e| ForepawError::ActionFailed(format!("OCR GetResults failed: {e}")))
}

/// Upscale RGBA pixels by an integer factor using Lanczos3 resampling.
///
/// Returns (upscaled_rgba, new_width, new_height).
fn upscale_rgba(rgba: &[u8], width: u32, height: u32, scale: u32) -> (Vec<u8>, u32, u32) {
    let img = match image::RgbaImage::from_raw(width, height, rgba.to_vec()) {
        Some(i) => i,
        None => return (rgba.to_vec(), width, height), // fallback: no upscale
    };
    let new_w = width * scale;
    let new_h = height * scale;
    let upscaled = image::imageops::resize(&img, new_w, new_h, image::imageops::FilterType::Lanczos3);
    (upscaled.into_raw(), new_w, new_h)
}
