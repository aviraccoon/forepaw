import XCTest

@testable import ForepawCore

final class CropRegionTests: XCTestCase {

    // MARK: - Basic crop calculation

    func testCropCenteredInWindow() {
        // Element at center of a 1000x800 window, window at origin 100,50
        let crop = CropRegion(
            screenRect: Rect(x: 400, y: 300, width: 200, height: 100),
            padding: 20
        )
        let result = crop.imageCropRect(
            windowOrigin: Point(x: 100, y: 50),
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // Window-relative: x=300-20=280, y=250-20=230, w=200+40=240, h=100+40=140
        // Scaled 2x: x=560, y=460, w=480, h=280
        XCTAssertEqual(result!.x, 560)
        XCTAssertEqual(result!.y, 460)
        XCTAssertEqual(result!.width, 480)
        XCTAssertEqual(result!.height, 280)
    }

    func testCropNoPadding() {
        let crop = CropRegion(
            screenRect: Rect(x: 200, y: 100, width: 300, height: 200),
            padding: 0
        )
        let result = crop.imageCropRect(
            windowOrigin: Point(x: 100, y: 50),
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // Window-relative: x=100, y=50, w=300, h=200
        // Scaled 2x: x=200, y=100, w=600, h=400
        XCTAssertEqual(result!.x, 200)
        XCTAssertEqual(result!.y, 100)
        XCTAssertEqual(result!.width, 600)
        XCTAssertEqual(result!.height, 400)
    }

    func testCropAt1xScale() {
        let crop = CropRegion(
            screenRect: Rect(x: 200, y: 100, width: 300, height: 200),
            padding: 0
        )
        let result = crop.imageCropRect(
            windowOrigin: Point(x: 100, y: 50),
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
            screenRect: Rect(x: 105, y: 55, width: 50, height: 30),
            padding: 20
        )
        let result = crop.imageCropRect(
            windowOrigin: Point(x: 100, y: 50),
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // Window-relative: x=5-20=-15, y=5-20=-15 -> clamped to 0,0
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
            screenRect: Rect(x: 1050, y: 810, width: 50, height: 30),
            padding: 20
        )
        let result = crop.imageCropRect(
            windowOrigin: Point(x: 100, y: 50),
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // Window-relative: x=950-20=930, y=760-20=740
        // Right: 930+90=1020 -> clamped to 1000
        // Bottom: 740+70=810 -> clamped to 800
        // width: 1000-930=70, height: 800-740=60
        XCTAssertEqual(result!.x, 1860)  // 930 * 2
        XCTAssertEqual(result!.y, 1480)  // 740 * 2
        XCTAssertEqual(result!.width, 140)  // 70 * 2
        XCTAssertEqual(result!.height, 120)  // 60 * 2
    }

    // MARK: - Outside window

    func testCropCompletelyOutsideReturnsNil() {
        // Element completely outside the window
        let crop = CropRegion(
            screenRect: Rect(x: 2000, y: 2000, width: 100, height: 50),
            padding: 20
        )
        let result = crop.imageCropRect(
            windowOrigin: Point(x: 100, y: 50),
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNil(result)
    }

    func testCropAboveWindowReturnsNil() {
        let crop = CropRegion(
            screenRect: Rect(x: 200, y: 0, width: 100, height: 10),
            padding: 5
        )
        let result = crop.imageCropRect(
            windowOrigin: Point(x: 100, y: 50),
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        // y=0-50-5=-55, bottom=-55+20=-35 -> clamped: 0..0 = no overlap
        XCTAssertNil(result)
    }

    // MARK: - Default padding

    func testDefaultPaddingIs20() {
        let crop = CropRegion(screenRect: Rect(x: 300, y: 200, width: 100, height: 50))
        XCTAssertEqual(crop.padding, 20)
    }

    // MARK: - Partial overlap

    func testCropPartialOverlapLeft() {
        // Element straddles left edge of window
        let crop = CropRegion(
            screenRect: Rect(x: 90, y: 200, width: 50, height: 30),
            padding: 0
        )
        let result = crop.imageCropRect(
            windowOrigin: Point(x: 100, y: 50),
            windowSize: Point(x: 1000, y: 800),
            scaleFactor: 2.0
        )
        XCTAssertNotNil(result)
        // Window-relative: x=-10, y=150, w=50, h=30
        // Clamped: x=0, right=min(1000,-10+50)=40, width=40
        XCTAssertEqual(result!.x, 0)
        XCTAssertEqual(result!.width, 80)  // 40 * 2
    }
}
