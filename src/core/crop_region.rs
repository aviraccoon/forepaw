/// Describes a crop region for area screenshots.
use crate::core::types::{Point, Rect};

pub struct CropRegion {
    pub rect: Rect,
    pub padding: f64,
}

impl CropRegion {
    pub fn new(rect: Rect, padding: f64) -> Self {
        Self { rect, padding }
    }

    /// Calculate the crop rectangle in image pixel coordinates.
    ///
    /// Returns (x, y, width, height) in pixels, or None if the region
    /// doesn't overlap the window.
    pub fn image_crop_rect(
        &self,
        window_size: &Point,
        scale_factor: f64,
    ) -> Option<(i64, i64, i64, i64)> {
        // Apply padding to window-relative coordinates
        let rel_x = self.rect.x - self.padding;
        let rel_y = self.rect.y - self.padding;
        let rel_w = self.rect.width + self.padding * 2.0;
        let rel_h = self.rect.height + self.padding * 2.0;

        // Clamp to window bounds
        let clamped_x = rel_x.max(0.0);
        let clamped_y = rel_y.max(0.0);
        let clamped_right = (rel_x + rel_w).min(window_size.x);
        let clamped_bottom = (rel_y + rel_h).min(window_size.y);

        let clamped_w = clamped_right - clamped_x;
        let clamped_h = clamped_bottom - clamped_y;

        if clamped_w <= 0.0 || clamped_h <= 0.0 {
            return None;
        }

        Some((
            (clamped_x * scale_factor).round() as i64,
            (clamped_y * scale_factor).round() as i64,
            (clamped_w * scale_factor).round() as i64,
            (clamped_h * scale_factor).round() as i64,
        ))
    }
}

impl Default for CropRegion {
    fn default() -> Self {
        Self::new(Rect::new(0.0, 0.0, 0.0, 0.0), 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_centered_in_window() {
        let crop = CropRegion::new(Rect::new(300.0, 250.0, 200.0, 100.0), 20.0);
        let result = crop
            .image_crop_rect(&Point::new(1000.0, 800.0), 2.0)
            .unwrap();
        assert_eq!(result.0, 560);
        assert_eq!(result.1, 460);
        assert_eq!(result.2, 480);
        assert_eq!(result.3, 280);
    }

    #[test]
    fn crop_no_padding() {
        let crop = CropRegion::new(Rect::new(100.0, 50.0, 300.0, 200.0), 0.0);
        let result = crop
            .image_crop_rect(&Point::new(1000.0, 800.0), 2.0)
            .unwrap();
        assert_eq!(result.0, 200);
        assert_eq!(result.1, 100);
        assert_eq!(result.2, 600);
        assert_eq!(result.3, 400);
    }

    #[test]
    fn crop_at_1x_scale() {
        let crop = CropRegion::new(Rect::new(100.0, 50.0, 300.0, 200.0), 0.0);
        let result = crop
            .image_crop_rect(&Point::new(1000.0, 800.0), 1.0)
            .unwrap();
        assert_eq!(result.0, 100);
        assert_eq!(result.1, 50);
        assert_eq!(result.2, 300);
        assert_eq!(result.3, 200);
    }

    #[test]
    fn crop_clamps_to_top_left() {
        let crop = CropRegion::new(Rect::new(5.0, 5.0, 50.0, 30.0), 20.0);
        let result = crop
            .image_crop_rect(&Point::new(1000.0, 800.0), 2.0)
            .unwrap();
        assert_eq!(result.0, 0);
        assert_eq!(result.1, 0);
        assert_eq!(result.2, 150); // 75 * 2
        assert_eq!(result.3, 110); // 55 * 2
    }

    #[test]
    fn crop_clamps_to_bottom_right() {
        let crop = CropRegion::new(Rect::new(950.0, 760.0, 50.0, 30.0), 20.0);
        let result = crop
            .image_crop_rect(&Point::new(1000.0, 800.0), 2.0)
            .unwrap();
        assert_eq!(result.0, 1860);
        assert_eq!(result.1, 1480);
        assert_eq!(result.2, 140);
        assert_eq!(result.3, 120);
    }

    #[test]
    fn crop_completely_outside() {
        let crop = CropRegion::new(Rect::new(2000.0, 2000.0, 100.0, 50.0), 20.0);
        assert!(crop
            .image_crop_rect(&Point::new(1000.0, 800.0), 2.0)
            .is_none());
    }

    #[test]
    fn crop_above_window() {
        let crop = CropRegion::new(Rect::new(100.0, -60.0, 100.0, 10.0), 5.0);
        assert!(crop
            .image_crop_rect(&Point::new(1000.0, 800.0), 2.0)
            .is_none());
    }

    #[test]
    fn default_padding_is_20() {
        let crop = CropRegion::default();
        assert!((crop.padding - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn crop_partial_overlap_left() {
        let crop = CropRegion::new(Rect::new(-10.0, 150.0, 50.0, 30.0), 0.0);
        let result = crop
            .image_crop_rect(&Point::new(1000.0, 800.0), 2.0)
            .unwrap();
        assert_eq!(result.0, 0);
        assert_eq!(result.1, 300); // 150 * 2
        assert_eq!(result.2, 80); // 40 * 2
    }
}
