//! Platform-agnostic geometric types.
//!
//! These replace platform-specific types (`CGPoint`, `CGRect`) so the core
//! crate stays dependency-free. Platform backends convert to/from their
//! native types at the boundary.

/// A point in 2D space (screen or window coordinates).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Point {
    /// X coordinate (screen-space or window-space).
    pub x: f64,
    /// Y coordinate (screen-space or window-space).
    pub y: f64,
}

impl Point {
    /// Create a new point.
    #[must_use]
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A rectangle with origin and size (screen or window coordinates).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Rect {
    /// Left edge X coordinate.
    pub x: f64,
    /// Top edge Y coordinate.
    pub y: f64,
    /// Width of the rectangle.
    pub width: f64,
    /// Height of the rectangle.
    pub height: f64,
}

impl Rect {
    /// Create a new rectangle.
    #[must_use]
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Translate this rect so its origin is relative to `origin`'s top-left:
    /// `screen_bounds.translate(window_origin)` yields bounds where `(0, 0)`
    /// is the window's top-left corner. Size is unchanged. Converts
    /// screen-absolute coordinates to window-relative.
    #[must_use]
    pub fn translate(self, origin: Self) -> Self {
        Self {
            x: self.x - origin.x,
            y: self.y - origin.y,
            width: self.width,
            height: self.height,
        }
    }

    /// Intersection of two rects, or `None` if they don't overlap.
    ///
    /// Edge/corner-touching rects (zero-width or zero-height overlap) count
    /// as non-overlapping.
    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        let width = right - x;
        let height = bottom - y;
        if width > 0.0 && height > 0.0 {
            Some(Self::new(x, y, width, height))
        } else {
            None
        }
    }

    /// Area (`width × height`). Dimensions are clamped to zero first, so a rect
    /// with negative width/height reports zero rather than a positive product.
    #[must_use]
    pub fn area(self) -> f64 {
        self.width.max(0.0) * self.height.max(0.0)
    }
}

/// Pixel dimensions of a raster image (width, height).
///
/// Used by [`ScreenshotResult`](crate::platform::ScreenshotResult) to report
/// the actual size of a captured image after any resampling, so consumers can
/// validate their scale math without decoding the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Dimensions {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Dimensions {
    /// Create a new dimensions value.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_new() {
        let p = Point::new(100.5, 200.75);
        assert!((p.x - 100.5).abs() < f64::EPSILON);
        assert!((p.y - 200.75).abs() < f64::EPSILON);
    }

    #[test]
    fn rect_new() {
        let r = Rect::new(10.0, 20.0, 800.0, 600.0);
        assert!((r.x - 10.0).abs() < f64::EPSILON);
        assert!((r.width - 800.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rect_translate_subtracts_origin() {
        // Element at screen (532, 342), window at (520, 244) -> (12, 98) relative.
        let screen = Rect::new(532.0, 342.0, 736.0, 33.0);
        let window = Rect::new(520.0, 244.0, 760.0, 720.0);
        let relative = screen.translate(window);
        assert!((relative.x - 12.0).abs() < 1e-9);
        assert!((relative.y - 98.0).abs() < 1e-9);
        assert!((relative.width - 736.0).abs() < f64::EPSILON);
        assert!((relative.height - 33.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rect_translate_origin_to_itself_is_zero() {
        // A window translated into its own space lands at the origin.
        let window = Rect::new(520.0, 244.0, 760.0, 720.0);
        let relative = window.translate(window);
        assert!((relative.x - 0.0).abs() < f64::EPSILON);
        assert!((relative.y - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rect_intersect_overlap() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);
        let inter = a.intersect(b).expect("overlapping rects intersect");
        assert!((inter.x - 50.0).abs() < f64::EPSILON);
        assert!((inter.y - 50.0).abs() < f64::EPSILON);
        assert!((inter.width - 50.0).abs() < f64::EPSILON);
        assert!((inter.height - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rect_intersect_disjoint_is_none() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(100.0, 100.0, 10.0, 10.0);
        assert!(a.intersect(b).is_none());
    }

    #[test]
    fn rect_intersect_edge_touch_is_none() {
        // Shared edge but zero-area overlap -> not intersecting.
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(10.0, 0.0, 10.0, 10.0);
        assert!(a.intersect(b).is_none());
    }

    #[test]
    fn rect_intersect_containment() {
        // A fully inside B -> intersection is A.
        let a = Rect::new(25.0, 25.0, 10.0, 10.0);
        let b = Rect::new(0.0, 0.0, 100.0, 100.0);
        let inter = a.intersect(b).expect("contained rect intersects");
        assert!((inter.x - 25.0).abs() < f64::EPSILON);
        assert!((inter.width - 10.0).abs() < f64::EPSILON);
        assert!((inter.height - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rect_area() {
        assert!((Rect::new(0.0, 0.0, 800.0, 600.0).area() - 480_000.0).abs() < f64::EPSILON);
        // Negative dims clamp to zero, never a positive product.
        assert!((Rect::new(0.0, 0.0, -5.0, 10.0).area() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dimensions_new() {
        let d = Dimensions::new(1920, 1080);
        assert_eq!(d.width, 1920);
        assert_eq!(d.height, 1080);
    }
}
