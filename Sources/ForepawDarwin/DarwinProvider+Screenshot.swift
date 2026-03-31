import ApplicationServices
import Cocoa
import ForepawCore

// MARK: - Screenshot & OCR

extension DarwinProvider {
    public func ocr(app: String?, window: String? = nil, find: String? = nil) async throws -> [OCRResult] {
        let screenshotResult = try await screenshot(app: app, window: window, annotate: false)
        guard let image = NSImage(contentsOfFile: screenshotResult.path),
            let rep = image.representations.first
        else {
            throw ForepawError.actionFailed("Failed to load screenshot at \(screenshotResult.path)")
        }
        let engine = OCREngine()
        let results = try engine.recognize(imagePath: screenshotResult.path, imageHeight: Double(rep.pixelsHigh))
        if let query = find {
            return engine.find(query, in: results)
        }
        return results
    }

    /// Click at a specific screen coordinate with app activation.
    public func clickAtPoint(_ point: CGPoint, app: String) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        try performMouseClick(at: point)
        return ActionResult(success: true, message: "clicked at \(Int(point.x)),\(Int(point.y))")
    }

    /// OCR-click: screenshot, find text, click at its position (with window offset).
    public func ocrClick(
        text: String, app: String, window: String? = nil,
        options: ClickOptions = .normal
    ) async throws -> ActionResult {
        let matches = try await ocr(app: app, window: window, find: text)
        guard let match = matches.first else {
            throw ForepawError.actionFailed("No text matching '\(text)' found on screen")
        }

        // OCR coordinates are in pixel space (Retina 2x).
        // CGEvent needs screen points. Divide by the display scale factor.
        let scaleFactor = NSScreen.main?.backingScaleFactor ?? 2.0

        // Also offset by window position (screen-space).
        let runningApp = try findApp(named: app)
        let resolved = try findWindow(pid: runningApp.processIdentifier, window: window)

        let screenPoint = CGPoint(
            x: match.center.x / scaleFactor + resolved.origin.x,
            y: match.center.y / scaleFactor + resolved.origin.y
        )

        let isRightClick = options.button == .right
        let isDoubleClick = options.clickCount > 1
        let label = isRightClick ? "right-clicked" : isDoubleClick ? "double-clicked" : "clicked"
        let cgButton: CGMouseButton = isRightClick ? .right : .left

        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        try performMouseClick(at: screenPoint, button: cgButton, clickCount: Int64(options.clickCount))
        return ActionResult(
            success: true, message: "\(label) '\(match.text)' at \(Int(screenPoint.x)),\(Int(screenPoint.y))")
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
            process.arguments = ["-x", "-l", String(resolved.windowID), path]
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
