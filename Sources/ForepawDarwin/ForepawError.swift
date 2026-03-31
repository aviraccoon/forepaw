import ForepawCore

public enum ForepawError: Error, CustomStringConvertible {
    case appNotFound(String)
    case staleRef(ElementRef)
    case actionFailed(String)
    case permissionDenied
    case screenRecordingDenied

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
        }
    }
}
