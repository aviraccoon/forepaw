import Foundation

/// Formats command results as JSON or plain text.
public struct OutputFormatter: Sendable {
    public let json: Bool

    public init(json: Bool = false) {
        self.json = json
    }

    public func format(success: Bool, command: String, data: [String: Any] = [:], error: OutputError? = nil) -> String {
        if json {
            return formatJSON(success: success, command: command, data: data, error: error)
        }
        if let error {
            return "error: \(error.message)" + (error.suggestion.map { "\nhint: \($0)" } ?? "")
        }
        if let text = data["text"] as? String {
            return text
        }
        return success ? "ok" : "failed"
    }

    private func formatJSON(success: Bool, command: String, data: [String: Any], error: OutputError?) -> String {
        // Simple JSON construction without Codable gymnastics for [String: Any]
        var pairs: [String] = [
            "\"ok\": \(success)",
            "\"command\": \"\(command)\"",
        ]
        if let error {
            var errorPairs = ["\"code\": \"\(error.code)\"", "\"message\": \"\(escapeJSON(error.message))\""]
            if let suggestion = error.suggestion {
                errorPairs.append("\"suggestion\": \"\(escapeJSON(suggestion))\"")
            }
            pairs.append("\"error\": {\(errorPairs.joined(separator: ", "))}")
        }
        return "{\(pairs.joined(separator: ", "))}"
    }

    private func escapeJSON(_ s: String) -> String {
        s.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
            .replacingOccurrences(of: "\n", with: "\\n")
            .replacingOccurrences(of: "\t", with: "\\t")
    }
}

public struct OutputError: Sendable {
    public let code: String
    public let message: String
    public let suggestion: String?

    public init(code: String, message: String, suggestion: String? = nil) {
        self.code = code
        self.message = message
        self.suggestion = suggestion
    }

    public static let permDenied = "PERM_DENIED"
    public static let appNotFound = "APP_NOT_FOUND"
    public static let elementNotFound = "ELEMENT_NOT_FOUND"
    public static let staleRef = "STALE_REF"
    public static let actionFailed = "ACTION_FAILED"
    public static let invalidArgs = "INVALID_ARGS"
}
