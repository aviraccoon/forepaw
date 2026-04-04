import Testing

@testable import ForepawCore

@Suite("OCROutput")
struct OCROutputTests {
    @Test("init with results and screenshot path")
    func initWithResultsAndPath() {
        let results = [
            OCRResult(text: "Hello", bounds: Rect(x: 10, y: 20, width: 50, height: 15)),
            OCRResult(text: "World", bounds: Rect(x: 70, y: 20, width: 50, height: 15)),
        ]
        let output = OCROutput(results: results, screenshotPath: "/tmp/test.jpg")
        #expect(output.results.count == 2)
        #expect(output.results[0].text == "Hello")
        #expect(output.results[1].text == "World")
        #expect(output.screenshotPath == "/tmp/test.jpg")
    }

    @Test("init with results only, no screenshot")
    func initWithResultsOnly() {
        let results = [
            OCRResult(text: "Test", bounds: Rect(x: 0, y: 0, width: 100, height: 20))
        ]
        let output = OCROutput(results: results)
        #expect(output.results.count == 1)
        #expect(output.screenshotPath == nil)
    }

    @Test("init with empty results and screenshot")
    func initEmptyResultsWithScreenshot() {
        let output = OCROutput(results: [], screenshotPath: "/tmp/empty.jpg")
        #expect(output.results.isEmpty)
        #expect(output.screenshotPath == "/tmp/empty.jpg")
    }

    @Test("init with empty results and no screenshot")
    func initEmptyResultsNoScreenshot() {
        let output = OCROutput(results: [])
        #expect(output.results.isEmpty)
        #expect(output.screenshotPath == nil)
    }
}
