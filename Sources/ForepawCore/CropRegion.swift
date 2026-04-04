/// Describes a crop region for area screenshots.
///
/// Coordinates are window-relative logical pixels (0,0 = window top-left).
/// The crop logic converts to image pixel coordinates using the scale factor.
public struct CropRegion: Sendable {
    /// The area to capture in window-relative coordinates (x, y, width, height).
    public let rect: Rect
    /// Padding in logical pixels to add around the region.
    public let padding: Double

    public init(rect: Rect, padding: Double = 20) {
        self.rect = rect
        self.padding = padding
    }

    /// Calculate the crop rectangle in image pixel coordinates.
    ///
    /// The captured image is window-relative (0,0 = window top-left) at
    /// `scaleFactor` resolution (e.g. 2x for Retina). This method adds padding,
    /// clamps to the window bounds, and scales to pixel coordinates.
    ///
    /// - Parameters:
    ///   - windowSize: Window size in logical pixels.
    ///   - scaleFactor: Image scale factor (2.0 for Retina captures).
    /// - Returns: Crop rectangle in image pixel coordinates (x, y, width, height),
    ///   or nil if the region doesn't overlap with the window.
    public func imageCropRect(
        windowSize: Point, scaleFactor: Double
    ) -> (x: Int, y: Int, width: Int, height: Int)? {
        // Apply padding to window-relative coordinates
        let relX = rect.x - padding
        let relY = rect.y - padding
        let relW = rect.width + padding * 2
        let relH = rect.height + padding * 2

        // Clamp to window bounds (0,0 to windowSize)
        let clampedX = max(0, relX)
        let clampedY = max(0, relY)
        let clampedRight = min(windowSize.x, relX + relW)
        let clampedBottom = min(windowSize.y, relY + relH)

        let clampedW = clampedRight - clampedX
        let clampedH = clampedBottom - clampedY

        guard clampedW > 0 && clampedH > 0 else { return nil }

        // Scale to image pixel coordinates
        return (
            x: Int(clampedX * scaleFactor),
            y: Int(clampedY * scaleFactor),
            width: Int(clampedW * scaleFactor),
            height: Int(clampedH * scaleFactor)
        )
    }
}
