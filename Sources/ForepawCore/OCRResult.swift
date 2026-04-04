/// OCR result: recognized text with its bounding box in image coordinates.
public struct OCRResult: Sendable {
    public let text: String
    public let bounds: Rect
    /// Center point of the recognized text region.
    public var center: (x: Double, y: Double) {
        (x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2)
    }

    public init(text: String, bounds: Rect) {
        self.text = text
        self.bounds = bounds
    }
}

/// Combined OCR output: recognized text results plus optional display screenshot.
public struct OCROutput: Sendable {
    /// Recognized text with coordinates.
    public let results: [OCRResult]
    /// Path to agent-friendly screenshot (JPEG 1x), if requested.
    public let screenshotPath: String?

    public init(results: [OCRResult], screenshotPath: String? = nil) {
        self.results = results
        self.screenshotPath = screenshotPath
    }
}
