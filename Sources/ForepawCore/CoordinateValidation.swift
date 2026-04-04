/// Validate that a point falls within a bounding rectangle.
/// Used to reject coordinate-based actions that would land outside the target window.
public enum CoordinateValidation {
    /// Check if a point is inside the given bounds.
    /// Returns nil if valid, or an error message string if outside.
    public static func validate(point: Point, bounds: Rect) -> String? {
        let minX = bounds.x
        let minY = bounds.y
        let maxX = bounds.x + bounds.width
        let maxY = bounds.y + bounds.height
        if point.x < minX || point.x > maxX || point.y < minY || point.y > maxY {
            let boundsStr = "\(Int(minX)),\(Int(minY)) \(Int(bounds.width))x\(Int(bounds.height))"
            return
                "coordinates \(Int(point.x)),\(Int(point.y)) are outside window bounds (\(boundsStr)). Re-snapshot to get current positions."
        }
        return nil
    }
}
