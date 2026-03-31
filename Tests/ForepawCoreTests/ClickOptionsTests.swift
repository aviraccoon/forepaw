import Testing

@testable import ForepawCore

@Suite("ClickOptions")
struct ClickOptionsTests {
    @Test("default is left single click")
    func defaultOptions() {
        let opts = ClickOptions()
        #expect(opts.button == .left)
        #expect(opts.clickCount == 1)
    }

    @Test("static presets")
    func presets() {
        let normal = ClickOptions.normal
        #expect(normal.button == .left)
        #expect(normal.clickCount == 1)

        let right = ClickOptions.rightClick
        #expect(right.button == .right)
        #expect(right.clickCount == 1)

        let double = ClickOptions.doubleClick
        #expect(double.button == .left)
        #expect(double.clickCount == 2)
    }

    @Test("custom combination")
    func customCombo() {
        let opts = ClickOptions(button: .right, clickCount: 3)
        #expect(opts.button == .right)
        #expect(opts.clickCount == 3)
    }

    @Test("MouseButton raw values")
    func mouseButtonRawValues() {
        #expect(MouseButton.left.rawValue == "left")
        #expect(MouseButton.right.rawValue == "right")
    }
}
