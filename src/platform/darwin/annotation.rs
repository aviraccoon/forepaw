//! Screenshot annotation rendering via CoreGraphics + CoreText.
//!
//! Renders numbered badges, labeled bounding boxes, spotlight overlays, and
//! coordinate grids onto screenshot images. Port of `AnnotationRenderer.swift`.

use std::ffi::CString;
use std::ptr;

use crate::core::annotation::{Annotation, AnnotationCategory, AnnotationStyle};
use crate::platform::darwin::app;
use crate::platform::darwin::ffi::{
    self, CGPointFFI, CGRectFFI, CGSizeFFI, CGColorRef, CGColorSpaceRef, CGContextRef,
    CTFontRef, CFAttributedStringRef,
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum AnnotationError {
    ImageLoadFailed(String),
    ContextCreationFailed,
    RenderFailed,
    SaveFailed(String),
}

impl std::fmt::Display for AnnotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImageLoadFailed(path) => write!(f, "Failed to load image: {path}"),
            Self::ContextCreationFailed => write!(f, "Failed to create graphics context"),
            Self::RenderFailed => write!(f, "Failed to render annotated image"),
            Self::SaveFailed(path) => write!(f, "Failed to save annotated image: {path}"),
        }
    }
}

impl std::error::Error for AnnotationError {}

// ---------------------------------------------------------------------------
// Color scheme
// ---------------------------------------------------------------------------

struct AnnotationColor {
    background: CGColorRef,
    border: CGColorRef,
    text: CGColorRef,
}

impl AnnotationColor {
    fn new(color_space: CGColorSpaceRef, bg: [f64; 4], bd: [f64; 4], tx: [f64; 4]) -> Self {
        Self {
            background: unsafe { ffi::CGColorCreate(color_space, bg.as_ptr()) },
            border: unsafe { ffi::CGColorCreate(color_space, bd.as_ptr()) },
            text: unsafe { ffi::CGColorCreate(color_space, tx.as_ptr()) },
        }
    }
}

fn category_color(color_space: CGColorSpaceRef, category: &AnnotationCategory) -> AnnotationColor {
    match category {
        AnnotationCategory::Button => AnnotationColor::new(
            color_space,
            [0.56, 0.93, 0.56, 0.9],
            [0.2, 0.65, 0.2, 1.0],
            [0.0, 0.2, 0.0, 1.0],
        ),
        AnnotationCategory::TextInput => AnnotationColor::new(
            color_space,
            [1.0, 0.84, 0.0, 0.9],
            [0.8, 0.6, 0.0, 1.0],
            [0.3, 0.2, 0.0, 1.0],
        ),
        AnnotationCategory::Selection => AnnotationColor::new(
            color_space,
            [0.53, 0.81, 0.92, 0.9],
            [0.2, 0.5, 0.7, 1.0],
            [0.0, 0.15, 0.3, 1.0],
        ),
        AnnotationCategory::Navigation => AnnotationColor::new(
            color_space,
            [0.87, 0.63, 0.87, 0.9],
            [0.6, 0.3, 0.6, 1.0],
            [0.25, 0.05, 0.25, 1.0],
        ),
        AnnotationCategory::Other => AnnotationColor::new(
            color_space,
            [1.0, 0.96, 0.56, 0.9],
            [0.7, 0.65, 0.3, 1.0],
            [0.3, 0.25, 0.0, 1.0],
        ),
    }
}

// ---------------------------------------------------------------------------
// CoreText helpers
// ---------------------------------------------------------------------------

/// Create a CTFont by name and size.
fn create_font(name: &str, size: f64) -> CTFontRef {
    let cf_name = app::cf_string_from_str(name);
    let font = unsafe { ffi::CTFontCreateWithName(cf_name as ffi::CFStringRef, size, ptr::null()) };
    unsafe { ffi::CFRelease(cf_name as ffi::CFTypeRef) };
    font
}

/// Create a CFAttributedString with font and optional color attributes.
fn create_attributed_string(
    text: &str,
    font: CTFontRef,
    color: Option<CGColorRef>,
) -> CFAttributedStringRef {
    let cf_text = app::cf_string_from_str(text);

    unsafe {
        let mut keys: Vec<*const std::ffi::c_void> = vec![ffi::kCTFontAttributeName as *const std::ffi::c_void];
        let mut values: Vec<*const std::ffi::c_void> = vec![font as *const std::ffi::c_void];

        if let Some(c) = color {
            keys.push(ffi::kCTForegroundColorAttributeName as *const std::ffi::c_void);
            values.push(c as *const std::ffi::c_void);
        }

        let attrs = ffi::CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            keys.len() as ffi::CFIndex,
            &ffi::kCFTypeDictionaryKeyCallBacks,
            &ffi::kCFTypeDictionaryValueCallBacks,
        );

        let attr_str = ffi::CFAttributedStringCreate(ptr::null(), cf_text as ffi::CFStringRef, attrs);
        ffi::CFRelease(attrs as ffi::CFTypeRef);
        ffi::CFRelease(cf_text as ffi::CFTypeRef);
        attr_str
    }
}

/// Measure the width and height of text rendered with a given font.
fn measure_text(text: &str, font: CTFontRef) -> (f64, f64) {
    let attr_str = create_attributed_string(text, font, None);
    let line = unsafe { ffi::CTLineCreateWithAttributedString(attr_str) };
    let bounds = unsafe { ffi::CTLineGetBoundsWithOptions(line, 0) };
    unsafe {
        ffi::CFRelease(line as ffi::CFTypeRef);
        ffi::CFRelease(attr_str as ffi::CFTypeRef);
    }
    (bounds.size.width.ceil(), bounds.size.height.ceil())
}

/// Draw text at a specific position in a CGContext.
fn draw_text(
    text: &str,
    ctx: CGContextRef,
    x: f64,
    y: f64,
    font: CTFontRef,
    color: CGColorRef,
) {
    let attr_str = create_attributed_string(text, font, Some(color));
    let line = unsafe { ffi::CTLineCreateWithAttributedString(attr_str) };
    unsafe {
        ffi::CGContextSaveGState(ctx);
        ffi::CGContextSetTextPosition(ctx, x, y);
        ffi::CTLineDraw(line, ctx);
        ffi::CGContextRestoreGState(ctx);
        ffi::CFRelease(line as ffi::CFTypeRef);
        ffi::CFRelease(attr_str as ffi::CFTypeRef);
    }
}

/// Truncate text with "..." to fit within maxWidth.
fn truncate_text(text: &str, font: CTFontRef, max_width: f64) -> String {
    let (w, _) = measure_text(text, font);
    if w <= max_width {
        return text.to_string();
    }

    // Binary search for the right truncation point
    let mut lo: usize = 0;
    let mut hi = text.len();
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let truncated = format!("{}...", &text[..mid]);
        let (tw, _) = measure_text(&truncated, font);
        if tw <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    format!("{}...", &text[..lo])
}

// ---------------------------------------------------------------------------
// Renderers
// ---------------------------------------------------------------------------

/// Render badge-style annotations (compact numbered pills at element positions).
fn render_badges(
    ctx: CGContextRef,
    annotations: &[Annotation],
    scale_factor: f64,
    color_space: CGColorSpaceRef,
) {
    let font = create_font("Helvetica-Bold", 11.0 * scale_factor);
    let padding = 3.0 * scale_factor;
    let corner_radius = 4.0 * scale_factor;
    let image_height = unsafe { ffi::CGBitmapContextGetHeight(ctx) } as f64;

    for annotation in annotations {
        let text = format!("{}", annotation.display_number);
        let (text_w, text_h) = measure_text(&text, font);
        let badge_w = text_w + padding * 2.0;
        let badge_h = text_h + padding * 2.0;

        // Position badge at top-left of element bounds (in pixel coordinates)
        let x = annotation.bounds.x * scale_factor;
        // CoreGraphics has origin at bottom-left; flip Y
        let y = image_height - (annotation.bounds.y * scale_factor) - badge_h;

        let badge_rect = CGRectFFI {
            origin: CGPointFFI { x, y },
            size: CGSizeFFI { width: badge_w, height: badge_h },
        };

        let color = category_color(color_space, &AnnotationCategory::from_role(&annotation.role));

        // Background pill
        let bg_path = unsafe {
            ffi::CGPathCreateWithRoundedRect(badge_rect, corner_radius, corner_radius, ptr::null())
        };
        unsafe {
            ffi::CGContextSetFillColorWithColor(ctx, color.background);
            ffi::CGContextAddPath(ctx, bg_path);
            ffi::CGContextFillPath(ctx);

            // Border
            ffi::CGContextSetStrokeColorWithColor(ctx, color.border);
            ffi::CGContextSetLineWidth(ctx, 1.0 * scale_factor);
            ffi::CGContextAddPath(ctx, bg_path);
            ffi::CGContextStrokePath(ctx);
        }

        // Text
        draw_text(&text, ctx, x + padding, y + padding, font, color.text);

        unsafe {
            ffi::CFRelease(bg_path as ffi::CFTypeRef);
            ffi::CFRelease(color.background as ffi::CFTypeRef);
            ffi::CFRelease(color.border as ffi::CFTypeRef);
            ffi::CFRelease(color.text as ffi::CFTypeRef);
        }
    }

    unsafe { ffi::CFRelease(font as ffi::CFTypeRef) };
}

/// Render labeled bounding boxes with role + name labels.
fn render_labeled(
    ctx: CGContextRef,
    annotations: &[Annotation],
    scale_factor: f64,
    color_space: CGColorSpaceRef,
) {
    let font = create_font("Helvetica", 10.0 * scale_factor);
    let bold_font = create_font("Helvetica-Bold", 10.0 * scale_factor);
    let padding = 2.0 * scale_factor;
    let corner_radius = 3.0 * scale_factor;
    let border_width = 1.5 * scale_factor;
    let image_height = unsafe { ffi::CGBitmapContextGetHeight(ctx) } as f64;

    for annotation in annotations {
        let color = category_color(color_space, &AnnotationCategory::from_role(&annotation.role));

        // Draw bounding box -- border only, no fill
        let box_rect = CGRectFFI {
            origin: CGPointFFI {
                x: annotation.bounds.x * scale_factor,
                y: image_height
                    - (annotation.bounds.y + annotation.bounds.height) * scale_factor,
            },
            size: CGSizeFFI {
                width: annotation.bounds.width * scale_factor,
                height: annotation.bounds.height * scale_factor,
            },
        };
        unsafe {
            ffi::CGContextSetStrokeColorWithColor(ctx, color.border);
            ffi::CGContextSetLineWidth(ctx, border_width);
            ffi::CGContextStrokeRect(ctx, box_rect);
        }

        // Compact label pill at top-left of bounding box
        let number_text = format!("{}", annotation.display_number);
        let rest_text = match &annotation.name {
            Some(name) if !name.is_empty() => {
                format!(" {} {}", annotation.short_role(), name)
            }
            _ => format!(" {}", annotation.short_role()),
        };
        let max_rest_width = 150.0 * scale_factor;
        let truncated_rest = truncate_text(&rest_text, font, max_rest_width);

        let (number_w, number_h) = measure_text(&number_text, bold_font);
        let (rest_w, rest_h) = measure_text(&truncated_rest, font);
        let label_w = number_w + rest_w + padding * 2.0;
        let label_h = number_h.max(rest_h) + padding * 2.0;

        // Place inside top-left corner of box
        let label_x = annotation.bounds.x * scale_factor;
        let label_y = image_height - (annotation.bounds.y * scale_factor) - label_h;

        let label_rect = CGRectFFI {
            origin: CGPointFFI { x: label_x, y: label_y },
            size: CGSizeFFI { width: label_w, height: label_h },
        };

        // Opaque background
        let label_path = unsafe {
            ffi::CGPathCreateWithRoundedRect(
                label_rect,
                corner_radius,
                corner_radius,
                ptr::null(),
            )
        };
        unsafe {
            ffi::CGContextSetFillColorWithColor(ctx, color.background);
            ffi::CGContextAddPath(ctx, label_path);
            ffi::CGContextFillPath(ctx);
            ffi::CFRelease(label_path as ffi::CFTypeRef);
        }

        draw_text(
            &number_text,
            ctx,
            label_x + padding,
            label_y + padding,
            bold_font,
            color.text,
        );
        draw_text(
            &truncated_rest,
            ctx,
            label_x + padding + number_w,
            label_y + padding,
            font,
            color.text,
        );

        unsafe {
            ffi::CFRelease(color.background as ffi::CFTypeRef);
            ffi::CFRelease(color.border as ffi::CFTypeRef);
            ffi::CFRelease(color.text as ffi::CFTypeRef);
        }
    }

    unsafe {
        ffi::CFRelease(font as ffi::CFTypeRef);
        ffi::CFRelease(bold_font as ffi::CFTypeRef);
    };
}

/// Render spotlight overlay (dims everything except annotated elements).
fn render_spotlight(
    ctx: CGContextRef,
    annotations: &[Annotation],
    scale_factor: f64,
    width: usize,
    height: usize,
    color_space: CGColorSpaceRef,
) {
    let image_height = height as f64;
    let image_width = width as f64;
    let full_rect = CGRectFFI {
        origin: CGPointFFI { x: 0.0, y: 0.0 },
        size: CGSizeFFI { width: image_width, height: image_height },
    };

    // Build a single path: full-screen rect + element holes.
    // Even-odd fill rule makes the holes transparent in the overlay.
    let overlay_path = unsafe { ffi::CGPathCreateMutable() };
    unsafe {
        ffi::CGPathAddRect(overlay_path, ptr::null(), full_rect);
    }

    let expansion = 3.0 * scale_factor;
    let corner_radius = 4.0 * scale_factor;
    for annotation in annotations {
        let rect = CGRectFFI {
            origin: CGPointFFI {
                x: annotation.bounds.x * scale_factor - expansion,
                y: image_height
                    - (annotation.bounds.y + annotation.bounds.height) * scale_factor
                    - expansion,
            },
            size: CGSizeFFI {
                width: annotation.bounds.width * scale_factor + expansion * 2.0,
                height: annotation.bounds.height * scale_factor + expansion * 2.0,
            },
        };
        unsafe {
            ffi::CGPathAddRoundedRect(overlay_path, ptr::null(), rect, corner_radius, corner_radius);
        }
    }

    // Fill with even-odd rule: overlay covers everything except the holes
    let overlay_color =
        unsafe { ffi::CGColorCreate(color_space, [0.0f64, 0.0, 0.0, 0.6].as_ptr()) };
    unsafe {
        ffi::CGContextSetFillColorWithColor(ctx, overlay_color);
        ffi::CGContextAddPath(ctx, overlay_path as ffi::CGPathRef);
        ffi::CGContextEOFillPath(ctx);
        ffi::CFRelease(overlay_path as ffi::CFTypeRef);
        ffi::CFRelease(overlay_color as ffi::CFTypeRef);
    }

    // Draw badges on the visible elements
    render_badges(ctx, annotations, scale_factor, color_space);
}

/// Render a coordinate grid on a screenshot.
///
/// Grid lines and axis labels use window-relative coordinates (logical pixels).
/// Tick marks appear every `spacing` points along each edge.
pub fn render_grid(
    image_path: &str,
    spacing: u32,
    scale_factor: f64,
    output_path: &str,
    origin_offset: (f64, f64),
) -> Result<(), AnnotationError> {
    let (width, height, ctx, color_space, image) = load_image_and_create_context(image_path)?;

    unsafe {
        ffi::CGContextDrawImage(
            ctx,
            CGRectFFI {
                origin: CGPointFFI { x: 0.0, y: 0.0 },
                size: CGSizeFFI {
                    width: width as f64,
                    height: height as f64,
                },
            },
            image,
        );
    }

    let image_height = height as f64;
    let image_width = width as f64;
    let font = create_font("Helvetica", 9.0 * scale_factor);

    // Colors
    let grid_color =
        unsafe { ffi::CGColorCreate(color_space, [1.0f64, 1.0, 1.0, 0.3].as_ptr()) };
    let label_bg_color =
        unsafe { ffi::CGColorCreate(color_space, [0.0f64, 0.0, 0.0, 0.6].as_ptr()) };
    let label_text_color =
        unsafe { ffi::CGColorCreate(color_space, [1.0f64, 1.0, 1.0, 0.9].as_ptr()) };

    let line_width = 1.0 * scale_factor;
    let offset_x = origin_offset.0;
    let offset_y = origin_offset.1;
    let spacing_f = spacing as f64;

    // First grid X: next multiple of spacing after offsetX
    let first_grid_x = ((offset_x / spacing_f).floor() + 1.0) * spacing_f;

    // Vertical lines + X labels along top edge
    let mut window_x = first_grid_x;
    while (window_x - offset_x) * scale_factor < image_width {
        let pixel_x = (window_x - offset_x) * scale_factor;

        unsafe {
            ffi::CGContextSetStrokeColorWithColor(ctx, grid_color);
            ffi::CGContextSetLineWidth(ctx, line_width);
            ffi::CGContextMoveToPoint(ctx, pixel_x, 0.0);
            ffi::CGContextAddLineToPoint(ctx, pixel_x, image_height);
            ffi::CGContextStrokePath(ctx);
        }

        let label = format!("{}", window_x as i64);
        let (tw, th) = measure_text(&label, font);
        let label_rect = CGRectFFI {
            origin: CGPointFFI {
                x: pixel_x - tw / 2.0 - 2.0 * scale_factor,
                y: image_height - th - 4.0 * scale_factor,
            },
            size: CGSizeFFI {
                width: tw + 4.0 * scale_factor,
                height: th + 2.0 * scale_factor,
            },
        };
        unsafe {
            ffi::CGContextSetFillColorWithColor(ctx, label_bg_color);
            ffi::CGContextFillRect(ctx, label_rect);
        }
        draw_text(
            &label,
            ctx,
            label_rect.origin.x + 2.0 * scale_factor,
            label_rect.origin.y + 1.0 * scale_factor,
            font,
            label_text_color,
        );

        window_x += spacing_f;
    }

    // Horizontal lines + Y labels along left edge
    let first_grid_y = ((offset_y / spacing_f).floor() + 1.0) * spacing_f;
    let mut window_y = first_grid_y;
    while (window_y - offset_y) * scale_factor < image_height {
        let pixel_y = (window_y - offset_y) * scale_factor;
        // CG origin is bottom-left
        let cg_y = image_height - pixel_y;

        unsafe {
            ffi::CGContextSetStrokeColorWithColor(ctx, grid_color);
            ffi::CGContextSetLineWidth(ctx, line_width);
            ffi::CGContextMoveToPoint(ctx, 0.0, cg_y);
            ffi::CGContextAddLineToPoint(ctx, image_width, cg_y);
            ffi::CGContextStrokePath(ctx);
        }

        let label = format!("{}", window_y as i64);
        let (tw, th) = measure_text(&label, font);
        let label_rect = CGRectFFI {
            origin: CGPointFFI {
                x: 2.0 * scale_factor,
                y: cg_y - th / 2.0 - 1.0 * scale_factor,
            },
            size: CGSizeFFI {
                width: tw + 4.0 * scale_factor,
                height: th + 2.0 * scale_factor,
            },
        };
        unsafe {
            ffi::CGContextSetFillColorWithColor(ctx, label_bg_color);
            ffi::CGContextFillRect(ctx, label_rect);
        }
        draw_text(
            &label,
            ctx,
            label_rect.origin.x + 2.0 * scale_factor,
            label_rect.origin.y + 1.0 * scale_factor,
            font,
            label_text_color,
        );

        window_y += spacing_f;
    }

    unsafe {
        ffi::CFRelease(grid_color as ffi::CFTypeRef);
        ffi::CFRelease(label_bg_color as ffi::CFTypeRef);
        ffi::CFRelease(label_text_color as ffi::CFTypeRef);
        ffi::CFRelease(font as ffi::CFTypeRef);
    }

    write_context_to_file(ctx, output_path, color_space, image)
}

// ---------------------------------------------------------------------------
// Main render entry point
// ---------------------------------------------------------------------------

/// Render annotations onto a screenshot image.
pub fn render(
    image_path: &str,
    annotations: &[Annotation],
    style: AnnotationStyle,
    scale_factor: f64,
    output_path: &str,
) -> Result<(), AnnotationError> {
    let (width, height, ctx, color_space, image) = load_image_and_create_context(image_path)?;

    // Draw original image
    unsafe {
        ffi::CGContextDrawImage(
            ctx,
            CGRectFFI {
                origin: CGPointFFI { x: 0.0, y: 0.0 },
                size: CGSizeFFI {
                    width: width as f64,
                    height: height as f64,
                },
            },
            image,
        );
    }

    match style {
        AnnotationStyle::Badges => {
            render_badges(ctx, annotations, scale_factor, color_space);
        }
        AnnotationStyle::Labeled => {
            render_labeled(ctx, annotations, scale_factor, color_space);
        }
        AnnotationStyle::Spotlight => {
            render_spotlight(ctx, annotations, scale_factor, width, height, color_space);
        }
    }

    write_context_to_file(ctx, output_path, color_space, image)
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Load a PNG image and create a bitmap context for rendering.
/// Returns (width, height, context, color_space, image) -- caller must release all.
fn load_image_and_create_context(
    image_path: &str,
) -> Result<(usize, usize, CGContextRef, CGColorSpaceRef, ffi::CGImageRef), AnnotationError> {
    let c_path = CString::new(image_path)
        .map_err(|_| AnnotationError::ImageLoadFailed(image_path.to_string()))?;

    unsafe {
        let data_provider = ffi::CGDataProviderCreateWithFilename(c_path.as_ptr());
        if data_provider.is_null() {
            return Err(AnnotationError::ImageLoadFailed(image_path.to_string()));
        }
        let image = ffi::CGImageCreateWithPNGDataProvider(data_provider, ptr::null(), 0, 0);
        ffi::CFRelease(data_provider as ffi::CFTypeRef);
        if image.is_null() {
            return Err(AnnotationError::ImageLoadFailed(image_path.to_string()));
        }

        let width = ffi::CGImageGetWidth(image);
        let height = ffi::CGImageGetHeight(image);
        let color_space = ffi::CGColorSpaceCreateDeviceRGB();

        // premultipliedLast = alpha is last component, premultiplied
        let ctx = ffi::CGBitmapContextCreate(
            ptr::null_mut(),
            width,
            height,
            8,
            0,
            color_space,
            CG_IMAGE_ALPHA_PREMULTIPLIED_LAST,
        );
        if ctx.is_null() {
            ffi::CFRelease(image as ffi::CFTypeRef);
            ffi::CFRelease(color_space as ffi::CFTypeRef);
            return Err(AnnotationError::ContextCreationFailed);
        }

        Ok((width, height, ctx, color_space, image))
    }
}

/// Finalize the bitmap context and write to a PNG file.
fn write_context_to_file(
    ctx: CGContextRef,
    output_path: &str,
    color_space: CGColorSpaceRef,
    image: ffi::CGImageRef,
) -> Result<(), AnnotationError> {
    unsafe {
        let output_image = ffi::CGBitmapContextCreateImage(ctx);
        if output_image.is_null() {
            ffi::CFRelease(ctx as ffi::CFTypeRef);
            ffi::CFRelease(image as ffi::CFTypeRef);
            ffi::CFRelease(color_space as ffi::CFTypeRef);
            return Err(AnnotationError::RenderFailed);
        }

        let output_url =
            objc2_foundation::NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(output_path));
        let png_type = objc2_foundation::NSString::from_str("public.png");
        let dest = ffi::CGImageDestinationCreateWithURL(
            objc2::rc::Retained::as_ptr(&output_url) as *const std::ffi::c_void,
            objc2::rc::Retained::as_ptr(&png_type) as *const std::ffi::c_void,
            1,
            ptr::null(),
        );

        let result = if dest.is_null() {
            Err(AnnotationError::SaveFailed(output_path.to_string()))
        } else {
            ffi::CGImageDestinationAddImage(dest, output_image, ptr::null());
            if ffi::CGImageDestinationFinalize(dest) == 0 {
                Err(AnnotationError::SaveFailed(output_path.to_string()))
            } else {
                Ok(())
            }
        };

        ffi::CFRelease(dest as ffi::CFTypeRef);
        ffi::CFRelease(output_image as ffi::CFTypeRef);
        ffi::CFRelease(ctx as ffi::CFTypeRef);
        ffi::CFRelease(image as ffi::CFTypeRef);
        ffi::CFRelease(color_space as ffi::CFTypeRef);

        result
    }
}

/// Bitmap context alpha info constant.
const CG_IMAGE_ALPHA_PREMULTIPLIED_LAST: u32 = 1;
