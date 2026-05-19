//! OCR via macOS Vision framework (`VNRecognizeTextRequest`).
//!
//! Uses objc2-vision bindings. Screenshots are captured via the screenshot
//! module, then Vision processes the image. Coordinates are converted from
//! Vision's normalized bottom-left-origin to window-relative top-left-origin.

use std::fmt::Write;

use objc2::AnyThread;
use objc2_core_foundation::CGRect;
use objc2_foundation::{NSDictionary, NSString};
use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequestTextRecognitionLevel};

use crate::core::errors::ForepawError;
use crate::core::ocr_result::{OCROutput, OCRResult};
use crate::core::types::Rect;
use crate::platform::darwin::app;
use crate::platform::darwin::ffi::{self, CGPointFFI};
use crate::platform::ScreenshotOptions;

/// Run OCR on an app window (or full screen).
///
/// Captures a 2x PNG screenshot for accuracy, runs Vision OCR,
/// converts coordinates to window-relative logical pixels.
///
/// # Errors
///
/// Returns [`ForepawError::ScreenRecordingDenied`] if screen recording permission is missing,
/// [`ForepawError::AppNotFound`] if the target application is not running,
/// or [`ForepawError::WindowNotFound`] if the window filter doesn't match.
pub fn ocr(
    app_name: Option<&str>,
    window: Option<&str>,
    find: Option<&str>,
    screenshot_options: Option<&ScreenshotOptions>,
) -> Result<OCROutput, ForepawError> {
    // Capture at 2x for accurate OCR
    let ocr_options = ScreenshotOptions {
        format: crate::platform::ImageFormat::Png,
        scale: 2,
        quality: 100,
        cursor: false,
    };

    let screenshot_params = crate::platform::ScreenshotParams {
        app: app_name,
        window,
        style: None,
        only: None,
        options: &ocr_options,
        crop: None,
        grid_spacing: None,
    };
    let screenshot_result = crate::platform::darwin::screenshot::screenshot(&screenshot_params)?;

    // Load the image using NSImage to get pixel dimensions
    let ns_image = objc2_app_kit::NSImage::initWithContentsOfFile(
        objc2_app_kit::NSImage::alloc(),
        &NSString::from_str(&screenshot_result.path),
    )
    .ok_or_else(|| {
        ForepawError::ActionFailed(format!(
            "Failed to load screenshot: {}",
            screenshot_result.path
        ))
    })?;

    let reps = ns_image.representations();
    let rep = reps
        .firstObject()
        .ok_or_else(|| ForepawError::ActionFailed("No image representations found".into()))?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "pixel height fits in f64 mantissa"
    )]
    let image_height = rep.pixelsHigh() as f64;

    let c_path = std::ffi::CString::new(screenshot_result.path.clone())
        .map_err(|_e| ForepawError::ActionFailed("Invalid screenshot path".into()))?;

    // Get CGImage from the NSImage representation
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "image load pipeline: open + decode + release"
    )]
    // SAFETY: CGDataProviderCreateWithFilename reads a local file and returns
    // an owned CFDataProvider. CGImageCreateWithPNGDataProvider consumes it.
    // CFRelease frees the provider. All pointers are checked for null before use.
    let cg_image = unsafe {
        let raw = ffi::CGDataProviderCreateWithFilename(c_path.as_ptr());
        if raw.is_null() {
            return Err(ForepawError::ActionFailed(
                "Failed to load image data".into(),
            ));
        }
        let img = ffi::CGImageCreateWithPNGDataProvider(raw, std::ptr::null(), 0, 0);
        ffi::CFRelease(raw as ffi::CFTypeRef);
        if img.is_null() {
            return Err(ForepawError::ActionFailed("Failed to decode PNG".into()));
        }
        img
    };

    // Run Vision OCR
    let raw_results = recognize_text(cg_image, image_height, find)?;

    // Release CGImage
    // SAFETY: cg_image is a valid CGImage we own from the block above.
    unsafe { ffi::CFRelease(cg_image as ffi::CFTypeRef) };

    // Convert image-pixel coordinates to window-relative logical pixels
    let scale_factor = crate::platform::darwin::screenshot::backing_scale_factor();
    let results = raw_results
        .into_iter()
        .map(|r| OCRResult {
            text: r.text,
            bounds: Rect::new(
                r.bounds.x / scale_factor,
                r.bounds.y / scale_factor,
                r.bounds.width / scale_factor,
                r.bounds.height / scale_factor,
            ),
        })
        .collect();

    // Optionally produce an agent-friendly display copy
    let display_path = if let Some(display_options) = screenshot_options {
        let tag = crate::platform::darwin::screenshot::temp_tag();
        Some(
            crate::platform::darwin::screenshot::post_process_screenshot(
                &screenshot_result.path,
                &tag,
                display_options,
                "",
            )?,
        )
    } else {
        std::fs::remove_file(&screenshot_result.path).ok();
        None
    };

    Ok(OCROutput {
        results,
        screenshot_path: display_path,
    })
}

/// Run `VNRecognizeTextRequest` on a `CGImage`.
/// Returns results in image-pixel coordinates (top-left origin).
fn recognize_text(
    cg_image: ffi::CGImageRef,
    image_height: f64,
    find: Option<&str>,
) -> Result<Vec<OCRResult>, ForepawError> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "image width fits in f64 mantissa"
    )]
    // SAFETY: CGImageGetWidth is a read-only accessor on a valid CGImage.
    let image_width = unsafe { ffi::CGImageGetWidth(cg_image) } as f64;

    // Create VNImageRequestHandler with the CGImage
    // We use objc2's wrapper which expects &CGImage from objc2-core-graphics.
    // Our raw CGImageRef is a pointer to CGImage, so we can transmute.
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "pointer cast + objc msg_send"
    )]
    // SAFETY: cg_image is a valid CGImageRef from CoreGraphics. The pointer
    // transmute is sound because CGImageRef is pointer-compatible with
    // objc2's CGImage. VNImageRequestHandler::initWithCGImage_options is an
    // objc2 msg_send that expects a valid CGImage reference.
    let handler = unsafe {
        let objc_cg_image: &objc2_core_graphics::CGImage =
            &*(cg_image as *const objc2_core_graphics::CGImage);
        let empty_options: &NSDictionary<NSString, objc2::runtime::AnyObject> =
            &NSDictionary::dictionary();
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            objc_cg_image,
            empty_options,
        )
    };

    // Create and configure the text recognition request
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(false);

    // Perform the request
    let request_ref: &objc2_vision::VNRequest = &request;
    let requests = objc2_foundation::NSArray::from_slice(&[request_ref]);
    handler
        .performRequests_error(&requests)
        .map_err(|e| ForepawError::ActionFailed(format!("Vision OCR failed: {e:?}")))?;

    let observations = request.results().unwrap_or_default();

    // Build block-level results from observations
    let mut results = Vec::new();
    for observation in &observations {
        let candidates = observation.topCandidates(1);
        let Some(candidate) = candidates.firstObject() else {
            continue;
        };
        let text = candidate.string().to_string();

        // Vision returns normalized coordinates (0-1) with origin at bottom-left.
        // Convert to top-left origin in pixel coordinates.
        // SAFETY: boundingBox() is an objc2 msg_send on a valid VNRecognizedTextObservation.
        let box_rect: CGRect = unsafe { observation.boundingBox() };
        let x = box_rect.origin.x * image_width;
        let y = image_height - (box_rect.origin.y + box_rect.size.height) * image_height;
        let width = box_rect.size.width * image_width;
        let height = box_rect.size.height * image_height;

        results.push(OCRResult {
            text,
            bounds: Rect::new(x, y, width, height),
        });
    }

    // Filter if searching
    if let Some(query) = find {
        let q = query.to_lowercase();
        let filtered: Vec<OCRResult> = results
            .into_iter()
            .filter(|r| r.text.to_lowercase().contains(&q))
            .collect();
        return Ok(filtered);
    }

    Ok(results)
}

/// Resolve OCR text to a window-relative center point.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the text is not found,
/// or multiple matches exist without an explicit index.
pub fn resolve_ocr_text(
    text: &str,
    app_name: &str,
    window: Option<&str>,
    index: Option<usize>,
) -> Result<(String, crate::core::types::Point), ForepawError> {
    let output = ocr(Some(app_name), window, Some(text), None)?;
    let matches = output.results;

    if matches.is_empty() {
        return Err(ForepawError::ActionFailed(format!(
            "No text matching '{text}' found on screen"
        )));
    }

    if matches.len() > 1 && index.is_none() {
        let mut listing = format!("Multiple matches for '{text}':\n");
        for (i, m) in matches.iter().enumerate() {
            writeln!(
                listing,
                "  --index {}: '{}' at {:.0},{:.0}",
                i + 1,
                m.text,
                m.center().0,
                m.center().1
            )
            .ok();
        }
        listing += "Use --index N to pick one.";
        return Err(ForepawError::ActionFailed(listing));
    }

    let resolved_index = (index.unwrap_or(1)).saturating_sub(1);
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
        crate::core::types::Point::new(match_result.center().0, match_result.center().1),
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
    app_name: &str,
    window: Option<&str>,
    options: &crate::core::key_combo::ClickOptions,
    index: Option<usize>,
) -> Result<crate::platform::ActionResult, ForepawError> {
    let (matched_text, window_point) = resolve_ocr_text(text, app_name, window, index)?;

    let (_, pid) = super::input::activate_app(app_name)?;
    let screen_point = app::to_screen_point(&window_point, pid)?;
    let cg_point = CGPointFFI {
        x: screen_point.x,
        y: screen_point.y,
    };

    super::input::perform_mouse_click(cg_point, options.button, options.click_count)?;

    let label = match options.button {
        crate::core::key_combo::MouseButton::Right => "right-clicked",
        _ if options.click_count > 1 => "double-clicked",
        crate::core::key_combo::MouseButton::Left => "clicked",
    };
    Ok(crate::platform::ActionResult::ok_msg(format!(
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
    app_name: &str,
    window: Option<&str>,
    index: Option<usize>,
) -> Result<crate::platform::ActionResult, ForepawError> {
    let (matched_text, window_point) = resolve_ocr_text(text, app_name, window, index)?;

    let (_, pid) = super::input::activate_app(app_name)?;
    let screen_point = app::to_screen_point(&window_point, pid)?;
    let cg_point = CGPointFFI {
        x: screen_point.x,
        y: screen_point.y,
    };
    super::input::move_mouse_to(cg_point)?;

    Ok(crate::platform::ActionResult::ok_msg(format!(
        "hovered '{matched_text}' at {:.0},{:.0}",
        window_point.x, window_point.y
    )))
}

/// Wait for text to appear on screen via OCR polling.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the text is not found before the timeout,
/// or [`ForepawError::ScreenRecordingDenied`] if screen recording permission is missing.
pub fn wait(
    text: &str,
    app_name: &str,
    window: Option<&str>,
    timeout: f64,
    interval: f64,
) -> Result<crate::platform::ActionResult, ForepawError> {
    let start = std::time::Instant::now();
    let timeout_dur = std::time::Duration::from_secs_f64(timeout);
    let interval_dur = std::time::Duration::from_secs_f64(interval);

    loop {
        match ocr(Some(app_name), window, Some(text), None) {
            Ok(output) if !output.results.is_empty() => {
                let Some(matched) = output.results.first() else {
                    continue;
                };
                return Ok(crate::platform::ActionResult::ok_msg(format!(
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
