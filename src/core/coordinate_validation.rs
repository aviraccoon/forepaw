/// Validate that a point falls within a window's bounds.
use crate::core::types::Point;

/// Check if a window-relative point is inside the window.
/// Returns None if valid, or an error message if outside.
pub fn validate(point: &Point, window_size: &Point) -> Option<String> {
    if point.x < 0.0 || point.x > window_size.x || point.y < 0.0 || point.y > window_size.y {
        let bounds_str = format!("{}x{}", window_size.x as i64, window_size.y as i64);
        Some(format!(
            "coordinates {},{} are outside window bounds (0,0 {bounds_str}). Re-snapshot to get current positions.",
            point.x as i64, point.y as i64
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window_size() -> Point {
        Point::new(1212.0, 756.0)
    }

    #[test]
    fn inside_window() {
        assert!(validate(&Point::new(100.0, 200.0), &window_size()).is_none());
    }

    #[test]
    fn at_origin() {
        assert!(validate(&Point::new(0.0, 0.0), &window_size()).is_none());
    }

    #[test]
    fn at_bottom_right() {
        assert!(validate(&Point::new(1212.0, 756.0), &window_size()).is_none());
    }

    #[test]
    fn negative_x() {
        let result = validate(&Point::new(-10.0, 400.0), &window_size()).unwrap();
        assert!(result.contains("-10,400"));
        assert!(result.contains("outside window bounds"));
        assert!(result.contains("1212x756"));
    }

    #[test]
    fn negative_y() {
        let result = validate(&Point::new(100.0, -5.0), &window_size()).unwrap();
        assert!(result.contains("100,-5"));
    }

    #[test]
    fn right_of_window() {
        let result = validate(&Point::new(1500.0, 400.0), &window_size()).unwrap();
        assert!(result.contains("1500,400"));
    }

    #[test]
    fn below_window() {
        assert!(validate(&Point::new(100.0, 800.0), &window_size()).is_some());
    }

    #[test]
    fn error_suggests_resnapshot() {
        let result = validate(&Point::new(-1.0, -1.0), &window_size()).unwrap();
        assert!(result.contains("Re-snapshot"));
    }

    #[test]
    fn small_window() {
        let size = Point::new(800.0, 600.0);
        assert!(validate(&Point::new(400.0, 300.0), &size).is_none());
        assert!(validate(&Point::new(-1.0, 300.0), &size).is_some());
        assert!(validate(&Point::new(801.0, 300.0), &size).is_some());
    }
}
