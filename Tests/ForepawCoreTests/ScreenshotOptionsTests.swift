import ForepawCore
import Testing

@Suite("ScreenshotOptions")
struct ScreenshotOptionsTests {
    @Test("default options are JPEG, q85, 1x, cursor on")
    func defaultOptions() {
        let opts = ScreenshotOptions.default
        #expect(opts.format == .jpeg)
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

    @Test("allCases includes both formats")
    func allCases() {
        #expect(ImageFormat.allCases.count == 2)
        #expect(ImageFormat.allCases.contains(.png))
        #expect(ImageFormat.allCases.contains(.jpeg))
    }
}
