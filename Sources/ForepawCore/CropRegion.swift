/// Describes a crop region for area screenshots.
///
/// All coordinates are in screen space (logical pixels). The crop logic
/// converts to image pixel coordinates using the window origin and scale factor.
public struct CropRegion: Sendable {
    /// The area to capture in screen coordinates (x, y, width, height).
    public let screenRect: Rect
    /// Padding in logical pixels to add around the region.
    public let padding: Double

    public init(screenRect: Rect, padding: Double = 20) {
        self.screenRect = screenRect
        self.padding = padding
    }

    /// Calculate the crop rectangle in image pixel coordinates.
    ///
    /// The captured image is window-relative (0,0 = window top-left) at
    /// `scaleFactor` resolution (e.g. 2x for Retina). This method converts
    /// the screen-space crop region to image pixel coordinates, adds padding,
    /// and clamps to the image bounds.
    ///
    /// - Parameters:
    ///   - windowOrigin: Window's top-left corner in screen coordinates.
    ///   - windowSize: Window size in logical pixels.
    ///   - scaleFactor: Image scale factor (2.0 for Retina captures).
    /// - Returns: Crop rectangle in image pixel coordinates (x, y, width, height),
    ///   or nil if the region doesn't overlap with the window.
    public func imageCropRect(
        windowOrigin: Point, windowSize: Point, scaleFactor: Double
    ) -> (x: Int, y: Int, width: Int, height: Int)? {
        // Convert screen coordinates to window-relative
        let relX = screenRect.x - windowOrigin.x - padding
        let relY = screenRect.y - windowOrigin.y - padding
        let relW = screenRect.width + padding * 2
        let relH = screenRect.height + padding * 2

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
