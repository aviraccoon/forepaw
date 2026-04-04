import ForepawCore
import Testing

@Suite("CoordinateValidation")
struct CoordinateValidationTests {
    let window = Rect(x: 553, y: 83, width: 1212, height: 756)

    @Test("point inside window returns nil")
    func insideWindow() {
        let result = CoordinateValidation.validate(
            point: Point(x: 700, y: 400), bounds: window)
        #expect(result == nil)
    }

    @Test("point at window origin is valid")
    func atOrigin() {
        let result = CoordinateValidation.validate(
            point: Point(x: 553, y: 83), bounds: window)
        #expect(result == nil)
    }

    @Test("point at window bottom-right edge is valid")
    func atBottomRight() {
        let result = CoordinateValidation.validate(
            point: Point(x: 553 + 1212, y: 83 + 756), bounds: window)
        #expect(result == nil)
    }

    @Test("point left of window returns error")
    func leftOfWindow() {
        let result = CoordinateValidation.validate(
            point: Point(x: 400, y: 400), bounds: window)
        #expect(result != nil)
        #expect(result!.contains("400,400"))
        #expect(result!.contains("outside window bounds"))
        #expect(result!.contains("553,83 1212x756"))
    }

    @Test("point above window returns error")
    func aboveWindow() {
        let result = CoordinateValidation.validate(
            point: Point(x: 700, y: 50), bounds: window)
        #expect(result != nil)
        #expect(result!.contains("700,50"))
    }

    @Test("point right of window returns error")
    func rightOfWindow() {
        let result = CoordinateValidation.validate(
            point: Point(x: 2000, y: 400), bounds: window)
        #expect(result != nil)
        #expect(result!.contains("2000,400"))
    }

    @Test("point below window returns error")
    func belowWindow() {
        let result = CoordinateValidation.validate(
            point: Point(x: 700, y: 1000), bounds: window)
        #expect(result != nil)
    }

    @Test("error message suggests re-snapshot")
    func errorSuggestsReSnapshot() {
        let result = CoordinateValidation.validate(
            point: Point(x: 0, y: 0), bounds: window)
        #expect(result!.contains("Re-snapshot"))
    }

    @Test("window at origin works")
    func windowAtOrigin() {
        let bounds = Rect(x: 0, y: 0, width: 800, height: 600)
        #expect(CoordinateValidation.validate(point: Point(x: 400, y: 300), bounds: bounds) == nil)
        #expect(CoordinateValidation.validate(point: Point(x: -1, y: 300), bounds: bounds) != nil)
    }
}
