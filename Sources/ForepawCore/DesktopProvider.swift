/// Platform abstraction for desktop automation.
///
/// macOS: implemented via AXUIElement + CGEvent (ForepawDarwin)
/// Linux: future implementation via AT-SPI2/DBus
///
/// The CLI target talks exclusively through this protocol. Any new public
/// method on a platform provider (e.g. DarwinProvider) must be added here
/// first -- the compiler enforces this because the CLI never imports the
/// platform target directly.
public protocol DesktopProvider: Sendable {

    // MARK: - Observation

    /// List running GUI applications.
    func listApps() async throws -> [AppInfo]

    /// List windows for an app, or all visible windows.
    func listWindows(app: String?) async throws -> [WindowInfo]

    /// Get the accessibility tree for an app window.
    func snapshot(app: String, options: SnapshotOptions) async throws -> ElementTree

    /// Take a screenshot. If `app` is nil, captures full screen.
    /// When `style` is provided, overlays annotations on interactive elements.
    /// When `only` is provided, only those refs are annotated.
    /// When `crop` is provided, the image is cropped to that region.
    func screenshot(
        app: String?, window: String?, style: AnnotationStyle?, only: [ElementRef]?,
        options: ScreenshotOptions, crop: CropRegion?
    ) async throws
        -> ScreenshotResult

    /// Screenshot + OCR, returning recognized text with screen coordinates.
    /// When `screenshotOptions` is provided, also saves an agent-friendly display copy.
    func ocr(
        app: String?, window: String?, find: String?,
        screenshotOptions: ScreenshotOptions?
    ) async throws -> OCROutput

    // MARK: - Actions (element-based)

    /// Click an element by ref, re-walking the tree in the target app.
    func click(ref: ElementRef, app: String, options: ClickOptions) async throws -> ActionResult

    /// Click at a screen coordinate.
    func clickAtPoint(_ point: Point, app: String, options: ClickOptions) async throws -> ActionResult

    /// Type text into an element (focuses via AX, then types).
    func type(ref: ElementRef, text: String, app: String) async throws -> ActionResult

    /// Set an element's value directly (bypasses keystroke simulation).
    func setValue(ref: ElementRef, value: String, app: String) async throws -> ActionResult

    // MARK: - Actions (text input)

    /// Type text into whatever is currently focused (no app activation).
    func keyboardType(text: String) async throws -> ActionResult

    /// Type text into the focused element in the target app.
    func keyboardType(text: String, app: String) async throws -> ActionResult

    /// Press a keyboard shortcut (global -- no app activation).
    func press(keys: KeyCombo) async throws -> ActionResult

    /// Press a keyboard shortcut in a specific app.
    func press(keys: KeyCombo, app: String) async throws -> ActionResult

    // MARK: - Actions (mouse)

    /// Hover over an element by ref (triggers tooltips, hover states).
    func hover(ref: ElementRef, app: String) async throws -> ActionResult

    /// Hover at a screen coordinate.
    func hoverAtPoint(_ point: Point, app: String?) async throws -> ActionResult

    /// Hover over text found via OCR.
    func ocrHover(
        text: String, app: String, window: String?, index: Int?
    ) async throws -> ActionResult

    /// Find text on screen via OCR and click it.
    func ocrClick(
        text: String, app: String, window: String?,
        options: ClickOptions, index: Int?
    ) async throws -> ActionResult

    /// Scroll within an app window.
    func scroll(
        direction: String, amount: Int, app: String,
        window: String?, ref: ElementRef?
    ) async throws -> ActionResult

    /// Drag along a path of screen coordinates.
    func drag(path: [Point], options: DragOptions, app: String?) async throws -> ActionResult

    /// Drag from one element to another.
    func drag(
        fromRef: ElementRef, toRef: ElementRef, app: String, options: DragOptions
    ) async throws -> ActionResult

    // MARK: - Actions (waiting)

    /// Wait for text to appear on screen via OCR polling.
    func wait(
        text: String, app: String, window: String?,
        timeout: Double, interval: Double
    ) async throws -> ActionResult

    // MARK: - Utility

    /// Resolve a ref to its center point in screen coordinates.
    func resolveRefPosition(_ ref: ElementRef, app: String) throws -> Point

    /// Resolve a ref to its bounding rectangle in screen coordinates.
    func resolveRefBounds(_ ref: ElementRef, app: String) throws -> Rect

    // MARK: - Permissions

    /// Check if accessibility permission is granted.
    func hasPermissions() -> Bool

    /// Check if screen recording permission is granted.
    func hasScreenRecordingPermission() -> Bool

    /// Request accessibility permission (shows system dialog).
    func requestPermissions() -> Bool

    /// Request screen recording permission (shows system dialog).
    func requestScreenRecordingPermission() -> Bool
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

public struct Point: Sendable, Codable {
    public let x: Double
    public let y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
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
    /// Default depth for AX tree walks. Shared between snapshot and resolveRef
    /// so refs are consistent across CLI invocations.
    public static let defaultDepth = 15

    public let interactiveOnly: Bool
    public let maxDepth: Int
    public let compact: Bool
    /// Skip the menu bar subtree (AXMenuBar). Saves significant time on apps
    /// with large menus (e.g. Music: 300+ menu items).
    public let skipMenuBar: Bool
    /// Skip subtrees rooted at elements with zero-size bounds (0x0).
    /// These are typically collapsed menus, hidden panels, or offscreen content.
    public let skipZeroSize: Bool

    public init(
        interactiveOnly: Bool = false, maxDepth: Int = defaultDepth,
        compact: Bool = false, skipMenuBar: Bool = false, skipZeroSize: Bool = false
    ) {
        self.interactiveOnly = interactiveOnly
        self.maxDepth = maxDepth
        self.compact = compact
        self.skipMenuBar = skipMenuBar
        self.skipZeroSize = skipZeroSize
    }
}

public struct ScreenshotResult: Sendable {
    public let path: String
    /// Structured annotation data, if annotations were requested.
    public let annotations: [Annotation]?
    /// If annotated, the text legend mapping labels to elements.
    public let legend: String?

    public init(path: String, annotations: [Annotation]?, legend: String?) {
        self.path = path
        self.annotations = annotations
        self.legend = legend
    }
}

/// Image format for screenshots.
public enum ImageFormat: String, Sendable, CaseIterable {
    case png
    case jpeg
    case webp

    /// File extension for this format.
    public var fileExtension: String {
        switch self {
        case .png: return "png"
        case .jpeg: return "jpg"
        case .webp: return "webp"
        }
    }

    /// Best available format on this system.
    /// Checks for WebP encoder (cwebp), falls back to JPEG.
    /// Result is cached for the process lifetime.
    public static var bestAvailable: ImageFormat {
        return EncoderDetection.bestFormat
    }
}

/// Options controlling screenshot output format and quality.
public struct ScreenshotOptions: Sendable {
    /// Image format (default: best available -- WebP if cwebp installed, else JPEG).
    public let format: ImageFormat
    /// Quality 1-100 (default 85). Applies to JPEG and WebP, ignored for PNG.
    public let quality: Int
    /// Output scale: 1 = logical pixels, 2 = Retina (default 1).
    public let scale: Int
    /// Include the mouse cursor in the screenshot.
    public let cursor: Bool

    public init(
        format: ImageFormat = .bestAvailable, quality: Int = 85,
        scale: Int = 1, cursor: Bool = true
    ) {
        self.format = format
        self.quality = quality
        self.scale = scale
        self.cursor = cursor
    }

    /// Default options optimized for agent use: best format, 1x, cursor visible.
    public static let `default` = ScreenshotOptions()

    /// Full quality: PNG, 2x Retina, cursor visible.
    public static let fullQuality = ScreenshotOptions(format: .png, scale: 2, cursor: true)
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

/// Options for drag operations.
public struct DragOptions: Sendable {
    public let steps: Int
    public let duration: Double
    public let modifiers: [KeyCombo.Modifier]
    public let pressure: Double?
    public let rightButton: Bool
    public let closePath: Bool

    public init(
        steps: Int = 30, duration: Double = 0.3,
        modifiers: [KeyCombo.Modifier] = [], pressure: Double? = nil,
        rightButton: Bool = false, closePath: Bool = false
    ) {
        self.steps = steps
        self.duration = duration
        self.modifiers = modifiers
        self.pressure = pressure
        self.rightButton = rightButton
        self.closePath = closePath
    }
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
