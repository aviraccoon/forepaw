//! Saliency detection for region click/hover targets.
//!
//! Finds the visual centroid of the most prominent element in an image region.
//! Uses pixel saturation to distinguish colored UI elements from desaturated
//! backgrounds, with brightness-deviation fallback for monochrome icons.

use std::ffi::CString;

use crate::core::types::{Point, Rect};
use crate::platform::darwin::ffi::{self, CGPointFFI, CGRectFFI, CGSizeFFI};

/// Find the centroid of high-saturation pixels in an image region.
///
/// Returns window-relative coordinates (accounting for region offset),
/// or `None` if no salient pixels are found.
pub fn find_target(image_path: &str, region: &Rect, scale_factor: f64) -> Option<Point> {
    let c_path = CString::new(image_path).ok()?;
    unsafe {
        let data_provider = ffi::CGDataProviderCreateWithFilename(c_path.as_ptr());
        if data_provider.is_null() {
            return None;
        }
        let image = ffi::CGImageCreateWithPNGDataProvider(data_provider, std::ptr::null(), 0, 0);
        ffi::CFRelease(data_provider as ffi::CFTypeRef);
        if image.is_null() {
            return None;
        }

        // Crop to the region (in pixel coordinates)
        let crop_rect = CGRectFFI {
            origin: CGPointFFI {
                x: region.x * scale_factor,
                y: region.y * scale_factor,
            },
            size: CGSizeFFI {
                width: region.width * scale_factor,
                height: region.height * scale_factor,
            },
        };
        let cropped = ffi::CGImageCreateWithImageInRect(image, crop_rect);
        ffi::CFRelease(image as ffi::CFTypeRef);

        let width = ffi::CGImageGetWidth(cropped);
        let height = ffi::CGImageGetHeight(cropped);
        if width == 0 || height == 0 {
            ffi::CFRelease(cropped as ffi::CFTypeRef);
            return None;
        }

        // Get raw pixel data (RGBA)
        let color_space = ffi::CGColorSpaceCreateDeviceRGB();
        let bytes_per_pixel: usize = 4;
        let bytes_per_row = bytes_per_pixel * width;
        let mut pixel_data = vec![0_u8; height * bytes_per_row];

        let ctx = ffi::CGBitmapContextCreate(
            pixel_data.as_mut_ptr() as *mut std::ffi::c_void,
            width,
            height,
            8,
            bytes_per_row,
            color_space,
            1, // CG_IMAGE_ALPHA_PREMULTIPLIED_LAST
        );
        if ctx.is_null() {
            ffi::CFRelease(color_space as ffi::CFTypeRef);
            ffi::CFRelease(cropped as ffi::CFTypeRef);
            return None;
        }

        ffi::CGContextDrawImage(
            ctx,
            CGRectFFI {
                origin: CGPointFFI { x: 0.0, y: 0.0 },
                size: CGSizeFFI {
                    width: width as f64,
                    height: height as f64,
                },
            },
            cropped,
        );

        // Compute saturation for each pixel and find centroid.
        // Also use brightness-based detection as fallback for white/gray icons on dark bg.
        let mut brightnesses = Vec::with_capacity(width * height);

        for y in 0..height {
            for x in 0..width {
                let offset = y * bytes_per_row + x * bytes_per_pixel;
                let r = f64::from(pixel_data[offset]) / 255.0;
                let g = f64::from(pixel_data[offset + 1]) / 255.0;
                let b = f64::from(pixel_data[offset + 2]) / 255.0;
                let max_c = r.max(g).max(b);
                let min_c = r.min(g).min(b);
                let brightness = f64::midpoint(max_c, min_c);
                brightnesses.push(brightness);
            }
        }

        brightnesses.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median_brightness = brightnesses[brightnesses.len() / 2];

        // Second pass: find salient pixels
        let sat_threshold = 0.25;
        let brightness_dev_threshold = 0.3;

        let mut sat_sum: f64 = 0.0;
        let mut sat_weighted_x: f64 = 0.0;
        let mut sat_weighted_y: f64 = 0.0;

        for y in 0..height {
            for x in 0..width {
                let offset = y * bytes_per_row + x * bytes_per_pixel;
                let r = f64::from(pixel_data[offset]) / 255.0;
                let g = f64::from(pixel_data[offset + 1]) / 255.0;
                let b = f64::from(pixel_data[offset + 2]) / 255.0;

                let max_c = r.max(g).max(b);
                let min_c = r.min(g).min(b);
                let brightness = f64::midpoint(max_c, min_c);

                // HSL saturation
                let saturation = if (max_c - min_c).abs() < f64::EPSILON {
                    0.0
                } else if brightness <= 0.5 {
                    (max_c - min_c) / (max_c + min_c)
                } else {
                    (max_c - min_c) / (2.0 - max_c - min_c)
                };

                let brightness_dev = (brightness - median_brightness).abs();

                let weight = if saturation >= sat_threshold {
                    saturation
                } else if brightness_dev >= brightness_dev_threshold {
                    brightness_dev * 0.5
                } else {
                    continue;
                };

                sat_sum += weight;
                sat_weighted_x += x as f64 * weight;
                sat_weighted_y += y as f64 * weight;
            }
        }

        ffi::CFRelease(ctx as ffi::CFTypeRef);
        ffi::CFRelease(color_space as ffi::CFTypeRef);
        ffi::CFRelease(cropped as ffi::CFTypeRef);

        if sat_sum <= 0.0 {
            return None;
        }

        // Centroid in pixel coordinates within the crop
        let centroid_px_x = sat_weighted_x / sat_sum;
        let centroid_px_y = sat_weighted_y / sat_sum;

        // Convert to window-relative logical coordinates
        Some(Point::new(
            region.x + centroid_px_x / scale_factor,
            region.y + centroid_px_y / scale_factor,
        ))
    }
}
