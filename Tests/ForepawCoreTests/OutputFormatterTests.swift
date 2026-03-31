import Testing

@testable import ForepawCore

@Suite("OutputFormatter")
struct OutputFormatterTests {
    @Test("plain text success with text data")
    func plainTextSuccess() {
        let formatter = OutputFormatter(json: false)
        let output = formatter.format(success: true, command: "click", data: ["text": "clicked"])
        #expect(output == "clicked")
    }

    @Test("plain text success without text data")
    func plainTextSuccessNoData() {
        let formatter = OutputFormatter(json: false)
        let output = formatter.format(success: true, command: "click")
        #expect(output == "ok")
    }

    @Test("plain text failure")
    func plainTextFailure() {
        let formatter = OutputFormatter(json: false)
        let output = formatter.format(success: false, command: "click")
        #expect(output == "failed")
    }

    @Test("plain text error with suggestion")
    func plainTextError() {
        let formatter = OutputFormatter(json: false)
        let error = OutputError(code: "STALE_REF", message: "Ref expired", suggestion: "Run snapshot again")
        let output = formatter.format(success: false, command: "click", error: error)

        #expect(output.contains("Ref expired"))
        #expect(output.contains("Run snapshot again"))
    }

    @Test("JSON success format")
    func jsonSuccess() {
        let formatter = OutputFormatter(json: true)
        let output = formatter.format(success: true, command: "click")

        #expect(output.contains("\"ok\": true"))
        #expect(output.contains("\"command\": \"click\""))
    }

    @Test("JSON error format")
    func jsonError() {
        let formatter = OutputFormatter(json: true)
        let error = OutputError(code: "APP_NOT_FOUND", message: "No such app")
        let output = formatter.format(success: false, command: "click", error: error)

        #expect(output.contains("\"ok\": false"))
        #expect(output.contains("\"code\": \"APP_NOT_FOUND\""))
        #expect(output.contains("\"message\": \"No such app\""))
    }

    @Test("JSON escapes special characters")
    func jsonEscaping() {
        let formatter = OutputFormatter(json: true)
        let error = OutputError(code: "ERR", message: "line1\nline2\twith \"quotes\"")
        let output = formatter.format(success: false, command: "test", error: error)

        #expect(output.contains("\\n"))
        #expect(output.contains("\\t"))
        #expect(output.contains("\\\"quotes\\\""))
    }
}
