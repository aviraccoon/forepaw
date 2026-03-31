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
