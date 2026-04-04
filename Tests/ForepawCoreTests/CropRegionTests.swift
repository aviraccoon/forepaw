import XCTest

@testable import ForepawCore

final class CropRegionTests: XCTestCase {

    // MARK: - Basic crop calculation

    func testCropCenteredInWindow() {
        // Element at (300,250) in window-relative coordinates, 1000x800 window
        let crop = CropRegion(
            rect: Rect(x: 300, y: 250, width: 200, height: 100),
            padding: 20
        )
        let result = crop.imageCropRect(
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // With padding: x=280, y=230, w=240, h=140
        // Scaled 2x: x=560, y=460, w=480, h=280
        XCTAssertEqual(result!.x, 560)
        XCTAssertEqual(result!.y, 460)
        XCTAssertEqual(result!.width, 480)
        XCTAssertEqual(result!.height, 280)
    }

    func testCropNoPadding() {
        let crop = CropRegion(
            rect: Rect(x: 100, y: 50, width: 300, height: 200),
            padding: 0
        )
        let result = crop.imageCropRect(
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // Scaled 2x: x=200, y=100, w=600, h=400
        XCTAssertEqual(result!.x, 200)
        XCTAssertEqual(result!.y, 100)
        XCTAssertEqual(result!.width, 600)
        XCTAssertEqual(result!.height, 400)
    }

    func testCropAt1xScale() {
        let crop = CropRegion(
            rect: Rect(x: 100, y: 50, width: 300, height: 200),
            padding: 0
        )
        let result = crop.imageCropRect(
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 1.0
        )
        XCTAssertNotNil(result)
        // At 1x: same as logical coordinates
        XCTAssertEqual(result!.x, 100)
        XCTAssertEqual(result!.y, 50)
        XCTAssertEqual(result!.width, 300)
        XCTAssertEqual(result!.height, 200)
    }

    // MARK: - Edge clamping

    func testCropClampsToTopLeft() {
        // Element near top-left, padding would extend outside window
        let crop = CropRegion(
            rect: Rect(x: 5, y: 5, width: 50, height: 30),
            padding: 20
        )
        let result = crop.imageCropRect(
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // With padding: x=5-20=-15, y=5-20=-15 -> clamped to 0,0
        // Right edge: -15+90=75, Bottom: -15+70=55 (within window)
        // Clamped: x=0, y=0, w=75, h=55
        XCTAssertEqual(result!.x, 0)
        XCTAssertEqual(result!.y, 0)
        XCTAssertEqual(result!.width, 150)  // 75 * 2
        XCTAssertEqual(result!.height, 110)  // 55 * 2
    }

    func testCropClampsToBottomRight() {
        // Element near bottom-right corner
        let crop = CropRegion(
            rect: Rect(x: 950, y: 760, width: 50, height: 30),
            padding: 20
        )
        let result = crop.imageCropRect(
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // With padding: x=930, y=740, w=90, h=70
        // Right: 930+90=1020 -> clamped to 1000, width=70
        // Bottom: 740+70=810 -> clamped to 800, height=60
        XCTAssertEqual(result!.x, 1860)  // 930 * 2
        XCTAssertEqual(result!.y, 1480)  // 740 * 2
        XCTAssertEqual(result!.width, 140)  // 70 * 2
        XCTAssertEqual(result!.height, 120)  // 60 * 2
    }

    // MARK: - Outside window

    func testCropCompletelyOutsideReturnsNil() {
        // Element completely outside the window
        let crop = CropRegion(
            rect: Rect(x: 2000, y: 2000, width: 100, height: 50),
            padding: 20
        )
        let result = crop.imageCropRect(
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNil(result)
    }

    func testCropAboveWindowReturnsNil() {
        let crop = CropRegion(
            rect: Rect(x: 100, y: -60, width: 100, height: 10),
            padding: 5
        )
        let result = crop.imageCropRect(
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        // y=-60-5=-65, bottom=-65+20=-45 -> clamped: 0..0 = no overlap
        XCTAssertNil(result)
    }

    // MARK: - Default padding

    func testDefaultPaddingIs20() {
        let crop = CropRegion(rect: Rect(x: 300, y: 200, width: 100, height: 50))
        XCTAssertEqual(crop.padding, 20)
    }

    // MARK: - Partial overlap

    func testCropPartialOverlapLeft() {
        // Element straddles left edge of window
        let crop = CropRegion(
            rect: Rect(x: -10, y: 150, width: 50, height: 30),
            padding: 0
        )
        let result = crop.imageCropRect(
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // x=-10, clamped to 0. Right edge: -10+50=40. Width=40
        XCTAssertEqual(result!.x, 0)
        XCTAssertEqual(result!.width, 80)  // 40 * 2
    }
}
