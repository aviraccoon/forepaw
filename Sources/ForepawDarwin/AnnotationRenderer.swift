import ApplicationServices
import Cocoa
import CoreText
import ForepawCore

/// Renders annotations onto a screenshot image using CoreGraphics.
public struct AnnotationRenderer: Sendable {
    public init() {}

    /// Render annotations onto a screenshot.
    ///
    /// - Parameters:
    ///   - imagePath: Path to the source screenshot PNG.
    ///   - annotations: Annotations with window-relative coordinates.
    ///   - style: Visual rendering style.
    ///   - scaleFactor: Retina scale factor (coordinates are in points, image is in pixels).
    ///   - outputPath: Where to save the annotated image.
    public func render(
        imagePath: String,
        annotations: [Annotation],
        style: AnnotationStyle,
        scaleFactor: CGFloat,
        outputPath: String
    ) throws {
        guard let dataProvider = CGDataProvider(filename: imagePath),
            let image = CGImage(
                pngDataProviderSource: dataProvider,
                decode: nil, shouldInterpolate: true,
                intent: .defaultIntent)
        else {
            throw AnnotationError.imageLoadFailed(imagePath)
        }

        let width = image.width
        let height = image.height
        let colorSpace = CGColorSpaceCreateDeviceRGB()

        guard
            let context = CGContext(
                data: nil,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: 0,
                space: colorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            )
        else {
            throw AnnotationError.contextCreationFailed
        }

        // Draw original image
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))

        switch style {
        case .badges:
            renderBadges(context: context, annotations: annotations, scaleFactor: scaleFactor)
        case .labeled:
            renderLabeled(context: context, annotations: annotations, scaleFactor: scaleFactor)
        case .spotlight:
            renderSpotlight(
                context: context, annotations: annotations, scaleFactor: scaleFactor,
                width: width, height: height)
        }

        guard let outputImage = context.makeImage() else {
            throw AnnotationError.renderFailed
        }

        let url = URL(fileURLWithPath: outputPath) as CFURL
        guard let destination = CGImageDestinationCreateWithURL(url, "public.png" as CFString, 1, nil) else {
            throw AnnotationError.saveFailed(outputPath)
        }
        CGImageDestinationAddImage(destination, outputImage, nil)
        guard CGImageDestinationFinalize(destination) else {
            throw AnnotationError.saveFailed(outputPath)
        }
    }

    // MARK: - Badge style (agent-optimized)

    private func renderBadges(context: CGContext, annotations: [Annotation], scaleFactor: CGFloat) {
        let fontSize: CGFloat = 11 * scaleFactor
        let font = CTFontCreateWithName("Helvetica-Bold" as CFString, fontSize, nil)
        let padding: CGFloat = 3 * scaleFactor
        let cornerRadius: CGFloat = 4 * scaleFactor

        for annotation in annotations {
            let text = "\(annotation.displayNumber)"
            let textSize = measureText(text, font: font)
            let badgeWidth = textSize.width + padding * 2
            let badgeHeight = textSize.height + padding * 2

            // Position badge at top-left of element bounds (in pixel coordinates)
            let x = annotation.bounds.x * scaleFactor
            // CoreGraphics has origin at bottom-left; flip Y
            let imageHeight = CGFloat(context.height)
            let y = imageHeight - (annotation.bounds.y * scaleFactor) - badgeHeight

            let badgeRect = CGRect(x: x, y: y, width: badgeWidth, height: badgeHeight)

            // Background pill
            let color = categoryColor(for: annotation.role)
            let bgPath = CGPath(
                roundedRect: badgeRect, cornerWidth: cornerRadius, cornerHeight: cornerRadius,
                transform: nil)
            context.setFillColor(color.background)
            context.addPath(bgPath)
            context.fillPath()

            // Border
            context.setStrokeColor(color.border)
            context.setLineWidth(1 * scaleFactor)
            context.addPath(bgPath)
            context.strokePath()

            // Text
            drawText(
                text, in: context, at: CGPoint(x: x + padding, y: y + padding),
                font: font, color: color.text)
        }
    }

    // MARK: - Labeled style (human-readable)

    private func renderLabeled(context: CGContext, annotations: [Annotation], scaleFactor: CGFloat) {
        let fontSize: CGFloat = 10 * scaleFactor
        let font = CTFontCreateWithName("Helvetica" as CFString, fontSize, nil)
        let boldFont = CTFontCreateWithName("Helvetica-Bold" as CFString, fontSize, nil)
        let padding: CGFloat = 2 * scaleFactor
        let cornerRadius: CGFloat = 3 * scaleFactor
        let borderWidth: CGFloat = 1.5 * scaleFactor
        let imageHeight = CGFloat(context.height)

        for annotation in annotations {
            let color = categoryColor(for: annotation.role)

            // Draw bounding box -- border only, no fill, to keep UI readable
            let boxRect = CGRect(
                x: annotation.bounds.x * scaleFactor,
                y: imageHeight - (annotation.bounds.y + annotation.bounds.height) * scaleFactor,
                width: annotation.bounds.width * scaleFactor,
                height: annotation.bounds.height * scaleFactor
            )
            context.setStrokeColor(color.border)
            context.setLineWidth(borderWidth)
            context.stroke(boxRect)

            // Compact label pill at top-left of bounding box
            let numberText = "\(annotation.displayNumber)"
            let restText: String
            if let name = annotation.name, !name.isEmpty {
                restText = " \(annotation.shortRole) \(name)"
            } else {
                restText = " \(annotation.shortRole)"
            }
            let maxRestWidth = 150 * scaleFactor
            let truncatedRest = truncateText(restText, font: font, maxWidth: maxRestWidth)

            let numberSize = measureText(numberText, font: boldFont)
            let restSize = measureText(truncatedRest, font: font)
            let labelWidth = numberSize.width + restSize.width + padding * 2
            let labelHeight = max(numberSize.height, restSize.height) + padding * 2

            // Place inside top-left corner of box
            let labelX = annotation.bounds.x * scaleFactor
            let labelY = imageHeight - (annotation.bounds.y * scaleFactor) - labelHeight

            let labelRect = CGRect(
                x: labelX, y: labelY,
                width: labelWidth, height: labelHeight)

            // Opaque background so it's readable over the UI
            let labelPath = CGPath(
                roundedRect: labelRect, cornerWidth: cornerRadius, cornerHeight: cornerRadius,
                transform: nil)
            context.setFillColor(color.background)
            context.addPath(labelPath)
            context.fillPath()

            drawText(
                numberText, in: context,
                at: CGPoint(x: labelX + padding, y: labelY + padding),
                font: boldFont, color: color.text)
            drawText(
                truncatedRest, in: context,
                at: CGPoint(x: labelX + padding + numberSize.width, y: labelY + padding),
                font: font, color: color.text)
        }
    }

    // MARK: - Spotlight style (focus mode)

    private func renderSpotlight(
        context: CGContext, annotations: [Annotation], scaleFactor: CGFloat,
        width: Int, height: Int
    ) {
        let imageHeight = CGFloat(height)
        let fullRect = CGRect(x: 0, y: 0, width: width, height: height)

        // Build a single path: full-screen rect + element holes.
        // Even-odd fill rule makes the holes transparent in the overlay.
        let overlayPath = CGMutablePath()
        overlayPath.addRect(fullRect)

        let expansion: CGFloat = 3 * scaleFactor
        let cornerRadius: CGFloat = 4 * scaleFactor
        for annotation in annotations {
            let rect = CGRect(
                x: annotation.bounds.x * scaleFactor - expansion,
                y: imageHeight - (annotation.bounds.y + annotation.bounds.height) * scaleFactor - expansion,
                width: annotation.bounds.width * scaleFactor + expansion * 2,
                height: annotation.bounds.height * scaleFactor + expansion * 2
            )
            overlayPath.addRoundedRect(in: rect, cornerWidth: cornerRadius, cornerHeight: cornerRadius)
        }

        // Fill with even-odd rule: the overlay covers everything except the holes
        context.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 0.6))
        context.addPath(overlayPath)
        context.fillPath(using: .evenOdd)

        // Draw badges on the visible elements
        renderBadges(context: context, annotations: annotations, scaleFactor: scaleFactor)
    }

    // MARK: - Color scheme

    private struct AnnotationColor {
        let background: CGColor
        let border: CGColor
        let text: CGColor
    }

    private func categoryColor(for role: String) -> AnnotationColor {
        let category = AnnotationCategory(role: role)
        switch category {
        case .button:
            return AnnotationColor(
                background: CGColor(red: 0.56, green: 0.93, blue: 0.56, alpha: 0.9),  // light green
                border: CGColor(red: 0.2, green: 0.65, blue: 0.2, alpha: 1.0),
                text: CGColor(red: 0, green: 0.2, blue: 0, alpha: 1.0)
            )
        case .textInput:
            return AnnotationColor(
                background: CGColor(red: 1.0, green: 0.84, blue: 0.0, alpha: 0.9),  // gold
                border: CGColor(red: 0.8, green: 0.6, blue: 0.0, alpha: 1.0),
                text: CGColor(red: 0.3, green: 0.2, blue: 0, alpha: 1.0)
            )
        case .selection:
            return AnnotationColor(
                background: CGColor(red: 0.53, green: 0.81, blue: 0.92, alpha: 0.9),  // sky blue
                border: CGColor(red: 0.2, green: 0.5, blue: 0.7, alpha: 1.0),
                text: CGColor(red: 0, green: 0.15, blue: 0.3, alpha: 1.0)
            )
        case .navigation:
            return AnnotationColor(
                background: CGColor(red: 0.87, green: 0.63, blue: 0.87, alpha: 0.9),  // plum
                border: CGColor(red: 0.6, green: 0.3, blue: 0.6, alpha: 1.0),
                text: CGColor(red: 0.25, green: 0.05, blue: 0.25, alpha: 1.0)
            )
        case .other:
            return AnnotationColor(
                background: CGColor(red: 1.0, green: 0.96, blue: 0.56, alpha: 0.9),  // light yellow
                border: CGColor(red: 0.7, green: 0.65, blue: 0.3, alpha: 1.0),
                text: CGColor(red: 0.3, green: 0.25, blue: 0, alpha: 1.0)
            )
        }
    }

    // MARK: - Text helpers

    private func measureText(_ text: String, font: CTFont) -> CGSize {
        let attributes: [NSAttributedString.Key: Any] = [.font: font]
        let attrString = NSAttributedString(string: text, attributes: attributes)
        let line = CTLineCreateWithAttributedString(attrString)
        let bounds = CTLineGetBoundsWithOptions(line, [])
        return CGSize(width: ceil(bounds.width), height: ceil(bounds.height))
    }

    private func drawText(
        _ text: String, in context: CGContext, at point: CGPoint,
        font: CTFont, color: CGColor
    ) {
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: color,
        ]
        let attrString = NSAttributedString(string: text, attributes: attributes)
        let line = CTLineCreateWithAttributedString(attrString)

        context.saveGState()
        context.textPosition = point
        CTLineDraw(line, context)
        context.restoreGState()
    }

    private func labelText(for annotation: Annotation) -> String {
        let name = annotation.name.map { " \($0)" } ?? ""
        return "[\(annotation.displayNumber)] \(annotation.shortRole)\(name)"
    }

    private func truncateText(_ text: String, font: CTFont, maxWidth: CGFloat) -> String {
        let size = measureText(text, font: font)
        if size.width <= maxWidth { return text }

        // Binary search for the right truncation point
        var lo = 0
        var hi = text.count
        while lo < hi {
            let mid = (lo + hi + 1) / 2
            let truncated = String(text.prefix(mid)) + "..."
            if measureText(truncated, font: font).width <= maxWidth {
                lo = mid
            } else {
                hi = mid - 1
            }
        }
        return String(text.prefix(lo)) + "..."
    }
}

/// Errors from the annotation renderer.
public enum AnnotationError: Error, CustomStringConvertible {
    case imageLoadFailed(String)
    case contextCreationFailed
    case renderFailed
    case saveFailed(String)

    public var description: String {
        switch self {
        case .imageLoadFailed(let path): return "Failed to load image: \(path)"
        case .contextCreationFailed: return "Failed to create graphics context"
        case .renderFailed: return "Failed to render annotated image"
        case .saveFailed(let path): return "Failed to save annotated image: \(path)"
        }
    }
}
