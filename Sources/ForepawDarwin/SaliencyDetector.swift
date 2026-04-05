import ApplicationServices
import Cocoa
import ForepawCore

/// Finds the visual centroid of the most prominent element in an image region.
/// Uses pixel saturation to distinguish colored UI elements (buttons, icons)
/// from desaturated backgrounds (gray, black, white).
public struct SaliencyDetector: Sendable {
    public init() {}

    /// Find the centroid of high-saturation pixels in an image.
    /// Returns window-relative coordinates (accounting for region offset).
    ///
    /// - Parameters:
    ///   - imagePath: Path to the screenshot PNG.
    ///   - region: Window-relative region to analyze (x, y, width, height).
    ///   - scaleFactor: Retina scale factor.
    /// - Returns: Window-relative (x, y) of the centroid, or nil if no salient pixels found.
    public func findTarget(
        imagePath: String,
        region: Rect,
        scaleFactor: CGFloat
    ) -> Point? {
        guard let dataProvider = CGDataProvider(filename: imagePath),
            let image = CGImage(
                pngDataProviderSource: dataProvider,
                decode: nil, shouldInterpolate: true,
                intent: .defaultIntent)
        else { return nil }

        // Crop to the region (in pixel coordinates)
        let cropRect = CGRect(
            x: region.x * scaleFactor,
            y: region.y * scaleFactor,
            width: region.width * scaleFactor,
            height: region.height * scaleFactor
        )
        guard let cropped = image.cropping(to: cropRect) else { return nil }

        let width = cropped.width
        let height = cropped.height
        guard width > 0 && height > 0 else { return nil }

        // Get raw pixel data (RGBA)
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bytesPerPixel = 4
        let bytesPerRow = bytesPerPixel * width
        var pixelData = [UInt8](repeating: 0, count: height * bytesPerRow)

        guard
            let context = CGContext(
                data: &pixelData,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: bytesPerRow,
                space: colorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            )
        else { return nil }

        context.draw(cropped, in: CGRect(x: 0, y: 0, width: width, height: height))

        // Compute saturation for each pixel and find centroid of high-saturation pixels.
        // Also try brightness-based detection as fallback (for white icons on dark bg).
        var satSum: Double = 0
        var satWeightedX: Double = 0
        var satWeightedY: Double = 0

        // First pass: compute median brightness to determine background type
        var brightnesses = [Double]()
        brightnesses.reserveCapacity(width * height)

        for y in 0..<height {
            for x in 0..<width {
                let offset = (y * bytesPerRow) + (x * bytesPerPixel)
                let r = Double(pixelData[offset]) / 255.0
                let g = Double(pixelData[offset + 1]) / 255.0
                let b = Double(pixelData[offset + 2]) / 255.0
                let maxC = max(r, g, b)
                let minC = min(r, g, b)
                let brightness = (maxC + minC) / 2.0
                brightnesses.append(brightness)
            }
        }

        brightnesses.sort()
        let medianBrightness = brightnesses[brightnesses.count / 2]

        // Second pass: find salient pixels
        // Strategy: use saturation primarily, but for desaturated icons (white/gray on dark bg),
        // use brightness deviation from the median
        let satThreshold: Double = 0.25
        let brightnessDevThreshold: Double = 0.3

        for y in 0..<height {
            for x in 0..<width {
                let offset = (y * bytesPerRow) + (x * bytesPerPixel)
                let r = Double(pixelData[offset]) / 255.0
                let g = Double(pixelData[offset + 1]) / 255.0
                let b = Double(pixelData[offset + 2]) / 255.0

                let maxC = max(r, g, b)
                let minC = min(r, g, b)
                let brightness = (maxC + minC) / 2.0

                // Saturation (HSL)
                let saturation: Double
                if maxC == minC {
                    saturation = 0
                } else if brightness <= 0.5 {
                    saturation = (maxC - minC) / (maxC + minC)
                } else {
                    saturation = (maxC - minC) / (2.0 - maxC - minC)
                }

                // Weight: high saturation OR strong brightness deviation from median
                let brightnessDev = abs(brightness - medianBrightness)
                let weight: Double
                if saturation >= satThreshold {
                    weight = saturation
                } else if brightnessDev >= brightnessDevThreshold {
                    weight = brightnessDev * 0.5  // lower weight than saturation
                } else {
                    continue
                }

                satSum += weight
                satWeightedX += Double(x) * weight
                satWeightedY += Double(y) * weight
            }
        }

        guard satSum > 0 else { return nil }

        // Centroid in pixel coordinates within the crop
        let centroidPxX = satWeightedX / satSum
        let centroidPxY = satWeightedY / satSum

        // Convert to window-relative logical coordinates
        let windowX = region.x + centroidPxX / scaleFactor
        let windowY = region.y + centroidPxY / scaleFactor

        return Point(x: windowX, y: windowY)
    }
}
