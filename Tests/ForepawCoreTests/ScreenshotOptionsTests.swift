import ForepawCore
import Testing

@Suite("ScreenshotOptions")
struct ScreenshotOptionsTests {
    @Test("default options are best available format, q85, 1x, cursor on")
    func defaultOptions() {
        let opts = ScreenshotOptions.default
        #expect(opts.format == .bestAvailable)
        #expect(opts.quality == 85)
        #expect(opts.scale == 1)
        #expect(opts.cursor == true)
    }

    @Test("fullQuality preset is PNG, 2x, cursor on")
    func fullQualityPreset() {
        let opts = ScreenshotOptions.fullQuality
        #expect(opts.format == .png)
        #expect(opts.scale == 2)
        #expect(opts.cursor == true)
    }

    @Test("custom values")
    func customValues() {
        let opts = ScreenshotOptions(format: .png, quality: 70, scale: 2, cursor: false)
        #expect(opts.format == .png)
        #expect(opts.quality == 70)
        #expect(opts.scale == 2)
        #expect(opts.cursor == false)
    }
}

@Suite("ImageFormat")
struct ImageFormatTests {
    @Test("raw values for CLI parsing")
    func rawValues() {
        #expect(ImageFormat(rawValue: "png") == .png)
        #expect(ImageFormat(rawValue: "jpeg") == .jpeg)
        #expect(ImageFormat(rawValue: "gif") == nil)
    }

    @Test("allCases includes all formats")
    func allCases() {
        #expect(ImageFormat.allCases.count == 3)
        #expect(ImageFormat.allCases.contains(.png))
        #expect(ImageFormat.allCases.contains(.jpeg))
        #expect(ImageFormat.allCases.contains(.webp))
    }

    @Test("file extensions")
    func fileExtensions() {
        #expect(ImageFormat.png.fileExtension == "png")
        #expect(ImageFormat.jpeg.fileExtension == "jpg")
        #expect(ImageFormat.webp.fileExtension == "webp")
    }

    @Test("bestAvailable returns jpeg or webp")
    func bestAvailable() {
        let best = ImageFormat.bestAvailable
        #expect(best == .jpeg || best == .webp)
    }
}
