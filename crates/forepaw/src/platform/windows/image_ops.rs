//! Shared in-memory RGBA transforms for the Windows backend.
//!
//! Both capture (`screenshot.rs`) and OCR (`ocr.rs`) need a Lanczos3 resize;
//! centralizing it avoids two copies of the same `imageops::resize` call.

use crate::core::types::Dimensions;

/// Resize an RGBA buffer to `dst` using Lanczos3 filtering.
///
/// Lanczos3 preserves sharp text edges in both directions: upscaling for OCR
/// accuracy, downscaling for logical-resolution screenshots.
///
/// Returns the resized pixels and their dimensions, or `None` if `rgba` does
/// not contain exactly `src.width × src.height × 4` bytes. Callers decide the
/// fallback: OCR returns the original buffer; screenshot capture errors.
#[must_use]
pub fn resize_rgba(rgba: &[u8], src: Dimensions, dst: Dimensions) -> Option<(Vec<u8>, Dimensions)> {
    let img = image::RgbaImage::from_raw(src.width, src.height, rgba.to_vec())?;
    let resized = image::imageops::resize(
        &img,
        dst.width,
        dst.height,
        image::imageops::FilterType::Lanczos3,
    );
    let dims = Dimensions::new(resized.width(), resized.height());
    Some((resized.into_raw(), dims))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `w × h` RGBA buffer filled with `pixel`.
    fn filled(w: u32, h: u32, pixel: [u8; 4]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(usize::try_from(w * h * 4).unwrap_or(0));
        for _ in 0..w * h {
            buf.extend_from_slice(&pixel);
        }
        buf
    }

    #[test]
    fn resize_rgba_returns_target_dims() {
        let rgba = filled(40, 30, [10, 20, 30, 255]);
        let (_, dims) =
            resize_rgba(&rgba, Dimensions::new(40, 30), Dimensions::new(20, 15)).expect("resize");
        assert_eq!(dims, Dimensions::new(20, 15));
    }

    #[test]
    fn resize_rgba_none_on_size_mismatch() {
        // Buffer claims 2×2 but only holds 1 pixel.
        let rgba = vec![0, 0, 0, 0];
        assert!(resize_rgba(&rgba, Dimensions::new(2, 2), Dimensions::new(4, 4)).is_none());
    }

    #[test]
    fn resize_rgba_preserves_uniform_color() {
        // A uniformly-colored source resized by an integer factor keeps that
        // color at every output pixel (Lanczos of a constant is the constant).
        let rgba = filled(10, 10, [200, 100, 50, 255]);
        let (out, _) =
            resize_rgba(&rgba, Dimensions::new(10, 10), Dimensions::new(5, 5)).expect("resize");
        for chunk in out.chunks_exact(4) {
            assert_eq!(chunk, [200, 100, 50, 255]);
        }
    }
}
