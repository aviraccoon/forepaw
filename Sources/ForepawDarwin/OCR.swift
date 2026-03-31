import Cocoa
import ForepawCore
import Vision

/// Runs macOS Vision framework OCR on a screenshot.
public struct OCREngine: Sendable {
    public init() {}

    /// Screenshot an app (or full screen) and run OCR. Returns recognized text with screen coordinates.
    /// When `find` is provided, uses word-level bounding boxes for precise substring targeting.
    public func recognize(imagePath: String, imageHeight: Double, find: String? = nil) throws -> [OCRResult] {
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

        let observations = request.results ?? []

        // If searching, try precise word-level matching first
        if let query = find {
            let precise = findPrecise(
                query, in: observations,
                imageWidth: cgImage.width, imageHeight: imageHeight)
            if !precise.isEmpty {
                return precise
            }
        }

        // Build block-level results
        let results = observations.compactMap { observation -> OCRResult? in
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

        if let query = find {
            // Fall back to block-level filtering
            return self.find(query, in: results)
        }
        return results
    }

    /// Find OCR results matching a query (case-insensitive substring).
    public func find(_ query: String, in results: [OCRResult]) -> [OCRResult] {
        let q = query.lowercased()
        return results.filter { $0.text.lowercased().contains(q) }
    }

    /// Find OCR results matching a query, with word-level bounding boxes.
    ///
    /// Uses Vision's `boundingBox(for:)` to get the bounding box of just the
    /// matched substring within a larger text block, giving precise coordinates
    /// for clicking individual words.
    public func findPrecise(
        _ query: String, in observations: [VNRecognizedTextObservation],
        imageWidth: Int, imageHeight: Double
    ) -> [OCRResult] {
        let q = query.lowercased()
        var results: [OCRResult] = []

        for observation in observations {
            guard let candidate = observation.topCandidates(1).first else { continue }
            let text = candidate.string
            let lowerText = text.lowercased()

            // Find all occurrences of the query in this text block
            var searchStart = lowerText.startIndex
            while let range = lowerText.range(of: q, range: searchStart..<lowerText.endIndex) {
                // Map to the original string's range
                let originalRange = Range(
                    uncheckedBounds: (
                        text.index(
                            text.startIndex,
                            offsetBy: lowerText.distance(from: lowerText.startIndex, to: range.lowerBound)),
                        text.index(
                            text.startIndex,
                            offsetBy: lowerText.distance(from: lowerText.startIndex, to: range.upperBound))
                    )
                )

                // Get the bounding box for just this substring
                if let box = try? candidate.boundingBox(for: originalRange) {
                    let rect = box.boundingBox
                    let x = rect.origin.x * Double(imageWidth)
                    let y = imageHeight - (rect.origin.y + rect.height) * imageHeight
                    let width = rect.width * Double(imageWidth)
                    let height = rect.height * imageHeight

                    results.append(
                        OCRResult(
                            text: String(text[originalRange]),
                            bounds: Rect(x: x, y: y, width: width, height: height)
                        ))
                }

                searchStart = range.upperBound
            }
        }

        return results
    }
}
