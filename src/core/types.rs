/// Platform-agnostic geometric types.
///
/// These replace platform-specific types (CGPoint, CGRect) so the core
/// crate stays dependency-free. Platform backends convert to/from their
/// native types at the boundary.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
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
