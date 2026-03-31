import Cocoa
import ForepawCore
import Vision

/// OCR result: recognized text with its bounding box in screen coordinates.
public struct OCRResult: Sendable {
    public let text: String
    public let bounds: Rect
    /// Center point for clicking.
    public var center: (x: Double, y: Double) {
        (x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2)
    }
}

/// Runs macOS Vision framework OCR on a screenshot.
public struct OCREngine: Sendable {
    public init() {}

    /// Screenshot an app (or full screen) and run OCR. Returns recognized text with screen coordinates.
    public func recognize(imagePath: String, imageHeight: Double) throws -> [OCRResult] {
        guard let image = NSImage(contentsOfFile: imagePath),
            let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
        else {
            throw ForepawError.actionFailed("Failed to load image at \(imagePath)")
        }

        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = false  // faster, preserves usernames/IDs

        let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
        try handler.perform([request])

        guard let observations = request.results else { return [] }

        return observations.compactMap { observation -> OCRResult? in
            guard let candidate = observation.topCandidates(1).first else { return nil }

            // Vision returns normalized coordinates (0-1) with origin at bottom-left.
            // Convert to screen coordinates (origin top-left).
            let box = observation.boundingBox
            let x = box.origin.x * Double(cgImage.width)
            let y = imageHeight - (box.origin.y + box.height) * imageHeight
            let width = box.width * Double(cgImage.width)
            let height = box.height * imageHeight

            return OCRResult(
                text: candidate.string,
                bounds: Rect(x: x, y: y, width: width, height: height)
            )
        }
    }

    /// Find OCR results matching a query (case-insensitive substring).
    public func find(_ query: String, in results: [OCRResult]) -> [OCRResult] {
        let q = query.lowercased()
        return results.filter { $0.text.lowercased().contains(q) }
    }
}
