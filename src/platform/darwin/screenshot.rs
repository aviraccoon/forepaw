//! Screenshot capture via `screencapture` CLI, with crop and post-processing.
//!
//! Uses the system `screencapture` binary for window/full-screen capture,
//! CoreGraphics for cropping, and `sips`/`cwebp` for format conversion.

use std::ffi::CString;
use std::fs;
use std::process::Command;

use crate::core::annotation::{AnnotationCollector, AnnotationLegend};
use crate::core::crop_region::CropRegion;
use crate::core::element_tree::ElementRef;
use crate::core::errors::ForepawError;
use crate::core::types::{Point, Rect};
use crate::platform::darwin::annotation;
use crate::platform::darwin::app;
use crate::platform::darwin::ffi::{self, CGPointFFI, CGRectFFI, CGSizeFFI};
use crate::platform::darwin::snapshot;
use crate::platform::ScreenshotResult;
use crate::platform::{ScreenshotOptions, ScreenshotParams, SnapshotOptions};

/// Generate a unique temp file tag for screenshot filenames.
pub fn temp_tag() -> String {
    crate::core::temp::temp_tag()
}

/// Run the system `screencapture` CLI to capture a window or full screen.
/// Returns the path to the captured PNG file.
fn capture_screenshot(window_id: Option<u32>, cursor: bool) -> Result<String, ForepawError> {
    let tag = temp_tag();
    let path = format!("/tmp/forepaw-{tag}.png");

    let mut args: Vec<String> = vec!["-x".into(), "-o".into()];
    if cursor {
        args.push("-C".into());
    }
    if let Some(wid) = window_id {
        args.push("-l".into());
        args.push(wid.to_string());
    }
    args.push(path.clone());

    let status = Command::new("/usr/sbin/screencapture")
        .args(&args)
        .status()
        .map_err(|e| ForepawError::ActionFailed(format!("failed to run screencapture: {e}")))?;

    if !status.success() {
        return Err(ForepawError::ActionFailed(format!(
            "screencapture exited with status {}",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(path)
}

/// Crop an image to a pixel rectangle using CoreGraphics.
fn crop_image(
    input_path: &str,
    output_path: &str,
    rect: (i32, i32, i32, i32),
) -> Result<(), ForepawError> {
    let c_path = CString::new(input_path)
        .map_err(|_| ForepawError::ActionFailed(format!("Invalid path: {input_path}")))?;
    unsafe {
        let data_provider = ffi::CGDataProviderCreateWithFilename(c_path.as_ptr());
        if data_provider.is_null() {
            return Err(ForepawError::ActionFailed(format!(
                "Failed to load image for cropping: {input_path}"
            )));
        }
        let image = ffi::CGImageCreateWithPNGDataProvider(data_provider, std::ptr::null(), 0, 0);
        ffi::CFRelease(data_provider as ffi::CFTypeRef);
        if image.is_null() {
            return Err(ForepawError::ActionFailed(format!(
                "Failed to decode PNG: {input_path}"
            )));
        }

        let img_w = ffi::CGImageGetWidth(image) as i32;
        let img_h = ffi::CGImageGetHeight(image) as i32;

        // Clamp rect to image bounds
        let cx = rect.0.clamp(0, img_w - 1);
        let cy = rect.1.clamp(0, img_h - 1);
        let cw = rect.2.clamp(1, img_w - cx);
        let ch = rect.3.clamp(1, img_h - cy);

        let crop_cg_rect = CGRectFFI {
            origin: CGPointFFI {
                x: f64::from(cx),
                y: f64::from(cy),
            },
            size: CGSizeFFI {
                width: f64::from(cw),
                height: f64::from(ch),
            },
        };

        let cropped = ffi::CGImageCreateWithImageInRect(image, crop_cg_rect);
        ffi::CFRelease(image as ffi::CFTypeRef);
        if cropped.is_null() {
            return Err(ForepawError::ActionFailed("Failed to crop image".into()));
        }

        // Write cropped image to output path
        let output_url = objc2_foundation::NSURL::fileURLWithPath(
            &objc2_foundation::NSString::from_str(output_path),
        );
        let png_type = objc2_foundation::NSString::from_str("public.png");
        let dest = {
            ffi::CGImageDestinationCreateWithURL(
                objc2::rc::Retained::as_ptr(&output_url) as *const std::ffi::c_void,
                objc2::rc::Retained::as_ptr(&png_type) as *const std::ffi::c_void,
                1,
                std::ptr::null(),
            )
        };
        if dest.is_null() {
            ffi::CFRelease(cropped as ffi::CFTypeRef);
            return Err(ForepawError::ActionFailed(format!(
                "Failed to create image destination: {output_path}"
            )));
        }

        ffi::CGImageDestinationAddImage(dest, cropped, std::ptr::null());
        let finalized = ffi::CGImageDestinationFinalize(dest);
        ffi::CFRelease(dest as ffi::CFTypeRef);
        ffi::CFRelease(cropped as ffi::CFTypeRef);

        if finalized == 0 {
            return Err(ForepawError::ActionFailed(format!(
                "Failed to write cropped image: {output_path}"
            )));
        }
        Ok(())
    }
}

/// Post-process a screenshot: downscale (1x) and/or convert format.
/// Returns the final output path.
pub fn post_process_screenshot(
    raw_path: &str,
    tag: &str,
    options: &ScreenshotOptions,
    suffix: &str,
) -> Result<String, ForepawError> {
    let needs_scale = options.scale == 1;
    let needs_format = options.format != crate::platform::ImageFormat::Png;

    if !needs_scale && !needs_format {
        return Ok(raw_path.to_string());
    }

    let ext = options.format.file_extension();
    let output_path = format!("/tmp/forepaw-{tag}{suffix}.{ext}");

    // WebP: scale with sips first if needed, then convert with cwebp
    if options.format == crate::platform::ImageFormat::Webp {
        let mut scaled_path = raw_path.to_string();

        if needs_scale {
            let target_width = image_pixel_width(raw_path)? / 2;
            if target_width > 0 {
                let scaled = format!("/tmp/forepaw-{tag}{suffix}-scaled.png");
                let status = Command::new("/usr/bin/sips")
                    .args([
                        "--resampleWidth",
                        &target_width.to_string(),
                        raw_path,
                        "--out",
                        &scaled,
                    ])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map_err(|e| ForepawError::ActionFailed(format!("sips failed: {e}")))?;

                if status.success() {
                    scaled_path = scaled;
                }
            }
        }

        let status = Command::new("/usr/bin/env")
            .args([
                "cwebp",
                "-q",
                &options.quality.to_string(),
                &scaled_path,
                "-o",
                &output_path,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| ForepawError::ActionFailed(format!("cwebp failed: {e}")))?;

        if !status.success() {
            return Err(ForepawError::ActionFailed(
                "cwebp failed. Is cwebp installed?".into(),
            ));
        }

        if scaled_path != raw_path {
            let _ = fs::remove_file(&scaled_path);
        }
        if output_path != raw_path {
            let _ = fs::remove_file(raw_path);
        }
        return Ok(output_path);
    }

    // JPEG: use sips for scale + format conversion
    let mut sips_args: Vec<String> = Vec::new();

    if needs_scale {
        let target_width = image_pixel_width(raw_path)? / 2;
        if target_width > 0 {
            sips_args.extend_from_slice(&["--resampleWidth".to_string(), target_width.to_string()]);
        }
    }

    if needs_format {
        sips_args.extend_from_slice(&[
            "-s".to_string(),
            "format".to_string(),
            "jpeg".to_string(),
            "-s".to_string(),
            "formatOptions".to_string(),
            options.quality.to_string(),
        ]);
    }

    sips_args.extend_from_slice(&[
        raw_path.to_string(),
        "--out".to_string(),
        output_path.clone(),
    ]);

    let status = Command::new("/usr/bin/sips")
        .args(&sips_args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| ForepawError::ActionFailed(format!("sips failed: {e}")))?;

    if !status.success() {
        return Ok(raw_path.to_string()); // fallback: return original
    }

    if output_path != raw_path {
        let _ = fs::remove_file(raw_path);
    }

    Ok(output_path)
}

/// Get the pixel width of an image file via CoreGraphics.
fn image_pixel_width(path: &str) -> Result<usize, ForepawError> {
    let c_path = CString::new(path.to_string())
        .map_err(|_| ForepawError::ActionFailed("Invalid path".into()))?;
    unsafe {
        let dp = ffi::CGDataProviderCreateWithFilename(c_path.as_ptr());
        if dp.is_null() {
            return Ok(0); // can't determine, let caller handle
        }
        let image = ffi::CGImageCreateWithPNGDataProvider(dp, std::ptr::null(), 0, 0);
        ffi::CFRelease(dp as ffi::CFTypeRef);
        if image.is_null() {
            return Ok(0);
        }
        let w = ffi::CGImageGetWidth(image);
        ffi::CFRelease(image as ffi::CFTypeRef);
        Ok(w)
    }
}

/// Apply a crop region to an image file.
/// Returns the path to the cropped image (or the original if crop doesn't overlap).
pub fn apply_crop(
    crop: &CropRegion,
    window_size: &Point,
    scale_factor: f64,
    input_path: &str,
    tag: &str,
    suffix: &str,
) -> Result<String, ForepawError> {
    let Some(crop_rect) = crop.image_crop_rect(window_size, scale_factor) else {
        return Ok(input_path.to_string());
    };
    let cropped_path = format!("/tmp/forepaw-{tag}{suffix}-cropped.png");
    crop_image(
        input_path,
        &cropped_path,
        (
            crop_rect.0 as i32,
            crop_rect.1 as i32,
            crop_rect.2 as i32,
            crop_rect.3 as i32,
        ),
    )?;
    let _ = fs::remove_file(input_path);
    Ok(cropped_path)
}

/// Get the main screen's Retina backing scale factor.
pub fn backing_scale_factor() -> f64 {
    // Access NSScreen.mainScreen via objc2
    use objc2_app_kit::NSScreen;
    let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
    let screen = NSScreen::mainScreen(mtm);
    match screen {
        Some(s) => s.backingScaleFactor(),
        None => 2.0, // default Retina assumption
    }
}

/// Take a screenshot of an app window (or full screen), with optional annotations.
///
/// This is the main entry point called from the DesktopProvider trait impl.
pub fn screenshot(params: &ScreenshotParams) -> Result<ScreenshotResult, ForepawError> {
    // Check screen recording permission
    if unsafe { ffi::CGPreflightScreenCaptureAccess() == 0 } {
        return Err(ForepawError::ScreenRecordingDenied);
    }

    let tag = temp_tag();
    let raw_path;

    let mut resolved_window: Option<app::ResolvedWindow> = None;

    if let Some(app_name) = params.app {
        let (_running_app, pid) = {
            let running_app = app::find_app(app_name)?;
            let pid = running_app.processIdentifier();
            #[allow(deprecated)]
            running_app.activateWithOptions(
                objc2_app_kit::NSApplicationActivationOptions::ActivateIgnoringOtherApps,
            );
            std::thread::sleep(std::time::Duration::from_millis(300));
            (running_app, pid)
        };

        let resolved = app::find_window(pid, params.window)?;
        raw_path = capture_screenshot(Some(resolved.window_id), params.options.cursor)?;
        resolved_window = Some(resolved);
    } else {
        raw_path = capture_screenshot(None, params.options.cursor)?;
    }

    // Non-annotated path: crop (if requested), grid, then post-process
    let Some(style) = params.style else {
        return render_plain(
            &raw_path,
            &tag,
            params.crop,
            resolved_window.as_ref(),
            params.grid_spacing,
            params.options,
        );
    };

    // Annotation requires an app name (for AX tree)
    let Some(app_name) = params.app else {
        return render_plain(
            &raw_path,
            &tag,
            params.crop,
            resolved_window.as_ref(),
            params.grid_spacing,
            params.options,
        );
    };

    // Get the AX tree for annotations
    let snapshot_opts = SnapshotOptions {
        interactive_only: true,
        max_depth: SnapshotOptions::DEFAULT_DEPTH,
        ..Default::default()
    };
    let tree = snapshot::snapshot(app_name, &snapshot_opts)?;

    // Determine window bounds for coordinate conversion
    let window_bounds = if let Some(resolved) = resolved_window.as_ref() {
        Rect::new(
            resolved.bounds.x,
            resolved.bounds.y,
            resolved.bounds.width,
            resolved.bounds.height,
        )
    } else {
        // Full screen fallback
        let screen = unsafe {
            let mtm = objc2::MainThreadMarker::new_unchecked();
            objc2_app_kit::NSScreen::mainScreen(mtm)
        };
        match screen {
            Some(s) => {
                let frame = s.frame();
                Rect::new(
                    frame.origin.x,
                    frame.origin.y,
                    frame.size.width,
                    frame.size.height,
                )
            }
            None => Rect::new(0.0, 0.0, 1440.0, 900.0),
        }
    };

    // Collect annotations
    let collector = AnnotationCollector::new();
    let mut annotations = collector.collect(&tree.root, window_bounds);

    // Filter to specific refs if requested
    if let Some(only) = params.only {
        if !only.is_empty() {
            let ref_set: std::collections::HashSet<ElementRef> = only.iter().copied().collect();
            annotations.retain(|a| ref_set.contains(&a.r#ref));
        }
    }

    if annotations.is_empty() {
        let current_path =
            apply_crop_if_needed(&raw_path, &tag, params.crop, resolved_window.as_ref())?;
        return Ok(ScreenshotResult {
            path: current_path,
            annotations: None,
            legend: Some("No interactive elements found".into()),
        });
    }

    // Render annotations on the full window image
    let annotated_path = format!("/tmp/forepaw-{tag}-annotated.png");
    let scale_factor = backing_scale_factor();
    annotation::render(
        &raw_path,
        &annotations,
        style,
        scale_factor,
        &annotated_path,
    )
    .map_err(|e| ForepawError::ActionFailed(e.to_string()))?;

    let _ = fs::remove_file(&raw_path);

    // Crop the annotated image if requested
    let mut current_annotated = annotated_path;
    if let Some(crop) = params.crop {
        if let Some(ref resolved) = resolved_window {
            let window_size = Point::new(resolved.bounds.width, resolved.bounds.height);
            current_annotated = apply_crop(
                crop,
                &window_size,
                scale_factor,
                &current_annotated,
                &tag,
                "-annotated",
            )?;
        }
    }

    // Generate legend
    let legend = AnnotationLegend::new().format(&annotations);

    // Post-process the annotated image (scale + format conversion)
    let final_path =
        post_process_screenshot(&current_annotated, &tag, params.options, "-annotated")?;

    Ok(ScreenshotResult {
        path: final_path,
        annotations: Some(annotations),
        legend: Some(legend),
    })
}

/// Non-annotated path: crop + grid + post-process.
fn render_plain(
    raw_path: &str,
    tag: &str,
    crop: Option<&CropRegion>,
    resolved_window: Option<&app::ResolvedWindow>,
    grid_spacing: Option<u32>,
    options: &ScreenshotOptions,
) -> Result<ScreenshotResult, ForepawError> {
    let mut current_path = raw_path.to_string();

    if let Some(crop) = crop {
        if let Some(resolved) = resolved_window {
            let window_size = Point::new(resolved.bounds.width, resolved.bounds.height);
            let scale = backing_scale_factor();
            current_path = apply_crop(crop, &window_size, scale, &current_path, tag, "")?;
        }
    }

    if let Some(spacing) = grid_spacing {
        let scale_factor = backing_scale_factor();
        let grid_path = format!("/tmp/forepaw-{tag}-grid.png");
        // Pass crop origin so grid labels show window-relative coords
        let crop_origin = crop.map(|c| (c.rect.x - c.padding, c.rect.y - c.padding));
        let origin_offset = crop_origin.unwrap_or((0.0, 0.0));
        annotation::render_grid(
            &current_path,
            spacing,
            scale_factor,
            &grid_path,
            origin_offset,
        )
        .map_err(|e| ForepawError::ActionFailed(e.to_string()))?;
        let _ = fs::remove_file(&current_path);
        current_path = grid_path;
    }

    let final_path = post_process_screenshot(&current_path, tag, options, "")?;
    Ok(ScreenshotResult {
        path: final_path,
        annotations: None,
        legend: None,
    })
}

/// Apply crop if needed, returning the (possibly new) path.
fn apply_crop_if_needed(
    raw_path: &str,
    tag: &str,
    crop: Option<&CropRegion>,
    resolved_window: Option<&app::ResolvedWindow>,
) -> Result<String, ForepawError> {
    match (crop, resolved_window) {
        (Some(crop), Some(resolved)) => {
            let window_size = Point::new(resolved.bounds.width, resolved.bounds.height);
            let scale = backing_scale_factor();
            apply_crop(crop, &window_size, scale, raw_path, tag, "")
        }
        _ => Ok(raw_path.to_string()),
    }
}
