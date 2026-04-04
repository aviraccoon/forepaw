import ForepawCore
import Testing

@Suite("CoordinateValidation")
struct CoordinateValidationTests {
    // Window is 1212x756. Coordinates are now window-relative (0,0 = top-left).
    let windowSize = Point(x: 1212, y: 756)

    @Test("point inside window returns nil")
    func insideWindow() {
        let result = CoordinateValidation.validate(
            point: Point(x: 100, y: 200), windowSize: windowSize)
        #expect(result == nil)
    }

    @Test("point at window origin is valid")
    func atOrigin() {
        let result = CoordinateValidation.validate(
            point: Point(x: 0, y: 0), windowSize: windowSize)
        #expect(result == nil)
    }

    @Test("point at window bottom-right edge is valid")
    func atBottomRight() {
        let result = CoordinateValidation.validate(
            point: Point(x: 1212, y: 756), windowSize: windowSize)
        #expect(result == nil)
    }

    @Test("negative x returns error")
    func negativeX() {
        let result = CoordinateValidation.validate(
            point: Point(x: -10, y: 400), windowSize: windowSize)
        #expect(result != nil)
        #expect(result!.contains("-10,400"))
        #expect(result!.contains("outside window bounds"))
        #expect(result!.contains("1212x756"))
    }

    @Test("negative y returns error")
    func negativeY() {
        let result = CoordinateValidation.validate(
            point: Point(x: 100, y: -5), windowSize: windowSize)
        #expect(result != nil)
        #expect(result!.contains("100,-5"))
    }

    @Test("point right of window returns error")
    func rightOfWindow() {
        let result = CoordinateValidation.validate(
            point: Point(x: 1500, y: 400), windowSize: windowSize)
        #expect(result != nil)
        #expect(result!.contains("1500,400"))
    }

    @Test("point below window returns error")
    func belowWindow() {
        let result = CoordinateValidation.validate(
            point: Point(x: 100, y: 800), windowSize: windowSize)
        #expect(result != nil)
    }

    @Test("error message suggests re-snapshot")
    func errorSuggestsReSnapshot() {
        let result = CoordinateValidation.validate(
            point: Point(x: -1, y: -1), windowSize: windowSize)
        #expect(result!.contains("Re-snapshot"))
    }

    @Test("small window works")
    func smallWindow() {
        let size = Point(x: 800, y: 600)
        #expect(CoordinateValidation.validate(point: Point(x: 400, y: 300), windowSize: size) == nil)
        #expect(CoordinateValidation.validate(point: Point(x: -1, y: 300), windowSize: size) != nil)
        #expect(CoordinateValidation.validate(point: Point(x: 801, y: 300), windowSize: size) != nil)
    }
}
