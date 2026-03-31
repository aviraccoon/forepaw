import ApplicationServices
import Cocoa
import ForepawCore

// MARK: - Screenshot & OCR

extension DarwinProvider {
    /// Screenshot + OCR, returning recognized text with screen coordinates.
    /// When `find` is provided, uses word-level bounding boxes for precise targeting.
    public func ocr(app: String?, window: String? = nil, find: String? = nil) async throws -> [OCRResult] {
        let screenshotResult = try await screenshot(app: app, window: window, annotate: false)
        guard let image = NSImage(contentsOfFile: screenshotResult.path),
            let rep = image.representations.first
        else {
            throw ForepawError.actionFailed("Failed to load screenshot at \(screenshotResult.path)")
        }
        let engine = OCREngine()
        return try engine.recognize(
            imagePath: screenshotResult.path, imageHeight: Double(rep.pixelsHigh), find: find)
    }

    /// Click at a specific screen coordinate with app activation.
    public func clickAtPoint(
        _ point: CGPoint, app: String, options: ClickOptions = .normal
    ) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        let button: CGMouseButton = options.button == .right ? .right : .left
        try performMouseClick(at: point, button: button, clickCount: Int64(options.clickCount))
        let isRight = options.button == .right
        let isDouble = options.clickCount > 1
        let label = isRight ? "right-clicked" : isDouble ? "double-clicked" : "clicked"
        return ActionResult(success: true, message: "\(label) at \(Int(point.x)),\(Int(point.y))")
    }

    /// OCR-click: screenshot, find text, click at its position (with window offset).
    /// Resolve OCR text to a screen point, handling multiple matches.
    ///
    /// - Parameters:
    ///   - text: Text to search for
    ///   - app: Target application name
    ///   - window: Optional window title or ID
    ///   - index: 1-based match index, or nil to require a unique match
    /// - Returns: Tuple of matched text and screen point
    internal func resolveOCRText(
        _ text: String, app: String, window: String? = nil, index: Int? = nil
    ) async throws -> (text: String, point: CGPoint) {
        let matches = try await ocr(app: app, window: window, find: text)
        guard !matches.isEmpty else {
            throw ForepawError.actionFailed("No text matching '\(text)' found on screen")
        }
        if matches.count > 1 && index == nil {
            let scaleFactor = NSScreen.main?.backingScaleFactor ?? 2.0
            let runningApp = try findApp(named: app)
            let resolved = try findWindow(pid: runningApp.processIdentifier, window: window)
            var listing = "Multiple matches for '\(text)':\n"
            for (i, m) in matches.enumerated() {
                let sx = Int(m.center.x / scaleFactor + resolved.origin.x)
                let sy = Int(m.center.y / scaleFactor + resolved.origin.y)
                listing += "  --index \(i + 1): '\(m.text)' at \(sx),\(sy)\n"
            }
            listing += "Use --index N to pick one."
            throw ForepawError.actionFailed(listing)
        }
        let resolvedIndex = (index ?? 1) - 1
        guard resolvedIndex >= 0 && resolvedIndex < matches.count else {
            throw ForepawError.actionFailed(
                "--index \(index ?? 0) out of range (\(matches.count) matches found)")
        }
        let match = matches[resolvedIndex]

        let scaleFactor = NSScreen.main?.backingScaleFactor ?? 2.0
        let runningApp = try findApp(named: app)
        let resolved = try findWindow(pid: runningApp.processIdentifier, window: window)
        let screenPoint = CGPoint(
            x: match.center.x / scaleFactor + resolved.origin.x,
            y: match.center.y / scaleFactor + resolved.origin.y
        )
        return (text: match.text, point: screenPoint)
    }

    public func ocrClick(
        text: String, app: String, window: String? = nil,
        options: ClickOptions = .normal, index: Int? = nil
    ) async throws -> ActionResult {
        let match = try await resolveOCRText(text, app: app, window: window, index: index)

        let isRightClick = options.button == .right
        let isDoubleClick = options.clickCount > 1
        let label = isRightClick ? "right-clicked" : isDoubleClick ? "double-clicked" : "clicked"
        let cgButton: CGMouseButton = isRightClick ? .right : .left

        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        try performMouseClick(at: match.point, button: cgButton, clickCount: Int64(options.clickCount))
        return ActionResult(
            success: true,
            message: "\(label) '\(match.text)' at \(Int(match.point.x)),\(Int(match.point.y))"
        )
    }

    /// Find a window for an app, optionally matching by title or window ID.
    ///
    /// Resolution order:
    /// 1. If `window` starts with "w-", match by CGWindowID
    /// 2. If `window` is provided, substring match against window titles
    /// 3. Otherwise, pick the largest non-phantom window (>= 10px)
    ///
    /// - Parameters:
    ///   - pid: The app's process identifier
    ///   - window: Optional window title substring or "w-<id>" identifier
    /// - Returns: The resolved window

    public func screenshot(app: String?, window: String? = nil, annotate: Bool) async throws -> ScreenshotResult {
        guard CGPreflightScreenCaptureAccess() else {
            throw ForepawError.screenRecordingDenied
        }
        let timestamp = Int(Date().timeIntervalSince1970)
        let path = "/tmp/forepaw-\(timestamp).png"

        if let appName = app {
            let runningApp = try findApp(named: appName)
            let resolved = try findWindow(pid: runningApp.processIdentifier, window: window)
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
            process.arguments = ["-x", "-o", "-l", String(resolved.windowID), path]
            try process.run()
            process.waitUntilExit()
        } else {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
            process.arguments = ["-x", path]
            try process.run()
            process.waitUntilExit()
        }

        // TODO: annotation support (overlay numbered labels on interactive elements)
        return ScreenshotResult(path: path, legend: annotate ? "annotation not yet implemented" : nil)
    }

}
