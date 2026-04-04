import ForepawCore
import Testing

@Suite("EncoderDetection")
struct EncoderDetectionTests {
    @Test("isAvailable finds common system tools")
    func findsSystemTools() {
        // /usr/bin/env always exists on macOS and Linux
        #expect(EncoderDetection.isAvailable("env"))
    }

    @Test("isAvailable returns false for nonexistent tools")
    func missingTool() {
        #expect(!EncoderDetection.isAvailable("forepaw-nonexistent-tool-xyz"))
    }

    @Test("bestFormat is jpeg or webp")
    func bestFormat() {
        let fmt = EncoderDetection.bestFormat
        #expect(fmt == .jpeg || fmt == .webp)
    }

    @Test("bestFormat matches cwebp availability")
    func bestFormatMatchesCwebp() {
        let hasCwebp = EncoderDetection.isAvailable("cwebp")
        let best = EncoderDetection.bestFormat
        if hasCwebp {
            #expect(best == .webp)
        } else {
            #expect(best == .jpeg)
        }
    }
}
