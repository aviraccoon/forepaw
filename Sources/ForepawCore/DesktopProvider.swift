/// Platform abstraction for desktop automation.
///
/// macOS: implemented via AXUIElement + CGEvent (ForepawDarwin)
/// Linux: future implementation via AT-SPI2/DBus
public protocol DesktopProvider: Sendable {
    /// List running GUI applications.
    func listApps() async throws -> [AppInfo]

    /// Get the accessibility tree for an app window.
    func snapshot(app: String, options: SnapshotOptions) async throws -> ElementTree

    /// Perform an action on an element by ref.
    func click(ref: ElementRef) async throws -> ActionResult

    /// Type text into an element.
    func type(ref: ElementRef, text: String) async throws -> ActionResult

    /// Set an element's value directly (bypasses keystroke simulation).
    func setValue(ref: ElementRef, value: String) async throws -> ActionResult

    /// Press a keyboard shortcut.
    func press(keys: KeyCombo) async throws -> ActionResult

    /// Take a screenshot. If `app` is nil, captures full screen.
    /// If `window` is provided, targets a specific window by title or ID.
    func screenshot(app: String?, window: String?, annotate: Bool) async throws -> ScreenshotResult

    /// List windows for an app, or all visible windows.
    func listWindows(app: String?) async throws -> [WindowInfo]
}

// MARK: - Supporting types

public struct AppInfo: Sendable, Codable {
    public let name: String
    public let bundleID: String?
    public let pid: Int32

    public init(name: String, bundleID: String?, pid: Int32) {
        self.name = name
        self.bundleID = bundleID
        self.pid = pid
    }
}

public struct WindowInfo: Sendable, Codable {
    public let id: String
    public let title: String
    public let app: String
    public let bounds: Rect?

    public init(id: String, title: String, app: String, bounds: Rect?) {
        self.id = id
        self.title = title
        self.app = app
        self.bounds = bounds
    }
}

public struct Rect: Sendable, Codable {
    public let x: Double
    public let y: Double
    public let width: Double
    public let height: Double

    public init(x: Double, y: Double, width: Double, height: Double) {
        self.x = x
        self.y = y
        self.width = width
        self.height = height
    }
}

public struct SnapshotOptions: Sendable {
    public let interactiveOnly: Bool
    public let maxDepth: Int
    public let compact: Bool

    public init(interactiveOnly: Bool = false, maxDepth: Int = 10, compact: Bool = false) {
        self.interactiveOnly = interactiveOnly
        self.maxDepth = maxDepth
        self.compact = compact
    }
}

public struct ScreenshotResult: Sendable {
    public let path: String
    /// If annotated, the text legend mapping labels to elements.
    public let legend: String?

    public init(path: String, legend: String?) {
        self.path = path
        self.legend = legend
    }
}

public struct ActionResult: Sendable {
    public let success: Bool
    public let message: String?

    public init(success: Bool, message: String? = nil) {
        self.success = success
        self.message = message
    }
}

/// Mouse button for click actions.
public enum MouseButton: String, Sendable {
    case left
    case right
}

/// Click behavior modifiers.
public struct ClickOptions: Sendable {
    public let button: MouseButton
    public let clickCount: Int

    public init(button: MouseButton = .left, clickCount: Int = 1) {
        self.button = button
        self.clickCount = clickCount
    }

    public static let normal = ClickOptions()
    public static let rightClick = ClickOptions(button: .right)
    public static let doubleClick = ClickOptions(clickCount: 2)
}

public struct KeyCombo: Sendable {
    public let key: String
    public let modifiers: [Modifier]

    public init(key: String, modifiers: [Modifier] = []) {
        self.key = key
        self.modifiers = modifiers
    }

    public enum Modifier: String, Sendable, CaseIterable {
        case command = "cmd"
        case shift
        case option = "opt"
        case control = "ctrl"
    }
}
