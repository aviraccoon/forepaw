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
}
