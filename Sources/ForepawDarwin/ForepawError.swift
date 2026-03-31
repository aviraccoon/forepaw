import ForepawCore

public enum ForepawError: Error, CustomStringConvertible {
    case appNotFound(String)
    case staleRef(ElementRef)
    case actionFailed(String)
    case permissionDenied
    case screenRecordingDenied
    case windowNotFound(String)
    case ambiguousWindow(String, String)

    public var description: String {
        switch self {
        case .appNotFound(let name):
            "Application not found: \(name). Run 'forepaw list-apps' to see running apps."
        case .staleRef(let ref):
            "Stale ref: \(ref). Run 'forepaw snapshot' to refresh refs, then retry."
        case .actionFailed(let msg):
            "Action failed: \(msg)"
        case .permissionDenied:
            "Accessibility permission not granted. Run 'forepaw permissions' to check."
        case .screenRecordingDenied:
            "Screen recording permission not granted. Run 'forepaw permissions' to check."
        case .windowNotFound(let query):
            "Window not found: \(query). Run 'forepaw list-windows --app <name>' to see windows."
        case .ambiguousWindow(let query, let matches):
            "Multiple windows match '\(query)'. Use --window with a more specific title or window ID:\n\(matches)"
        }
    }
}
