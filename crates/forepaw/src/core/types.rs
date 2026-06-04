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
}
