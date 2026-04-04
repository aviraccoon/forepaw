/// Validate that a point falls within a window's bounds.
/// Used to reject coordinate-based actions that would land outside the target window.
public enum CoordinateValidation {
    /// Check if a window-relative point is inside the window.
    ///
    /// Coordinates are window-relative: (0,0) = window top-left.
    /// Valid range is 0..width, 0..height.
    ///
    /// - Parameters:
    ///   - point: Window-relative coordinates.
    ///   - windowSize: Window width and height.
    /// - Returns: nil if valid, or an error message string if outside.
    public static func validate(point: Point, windowSize: Point) -> String? {
        if point.x < 0 || point.x > windowSize.x || point.y < 0 || point.y > windowSize.y {
            let boundsStr = "\(Int(windowSize.x))x\(Int(windowSize.y))"
            return
                "coordinates \(Int(point.x)),\(Int(point.y)) are outside window bounds (0,0 \(boundsStr)). Re-snapshot to get current positions."
        }
        return nil
    }
}
