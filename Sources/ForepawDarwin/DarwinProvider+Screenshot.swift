import ApplicationServices
import Cocoa
import ForepawCore

// MARK: - Screenshot & OCR

extension DarwinProvider {
    /// Screenshot + OCR, returning recognized text with screen coordinates.
    /// When `find` is provided, uses word-level bounding boxes for precise targeting.
    public func ocr(app: String?, window: String? = nil, find: String? = nil) async throws -> [OCRResult] {
        // OCR needs full-res PNG for accurate text recognition
        let ocrOptions = ScreenshotOptions(format: .png, scale: 2, cursor: false)
        let screenshotResult = try await screenshot(
            app: app, window: window, style: nil, only: nil, options: ocrOptions)
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
        _ point: Point, app: String, options: ClickOptions = .normal
    ) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)

        // Reject coordinates outside the window -- a misplaced click could be destructive
        try validatePointInWindow(point, pid: runningApp.processIdentifier)
        let cgPoint = CGPoint(x: point.x, y: point.y)

        let button: CGMouseButton = options.button == .right ? .right : .left
        try performMouseClick(at: cgPoint, button: button, clickCount: Int64(options.clickCount))
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

    public func screenshot(
        app: String?, window: String? = nil, style: AnnotationStyle? = nil, only: [ElementRef]? = nil,
        options: ScreenshotOptions = .default
    ) async throws -> ScreenshotResult {
        guard CGPreflightScreenCaptureAccess() else {
            throw ForepawError.screenRecordingDenied
        }
        let tag = "\(Int(Date().timeIntervalSince1970))-\(UInt32.random(in: 0...0xFFFF))"
        // Always capture as PNG first (needed for annotations, OCR, and clean downscaling)
        let rawPath = "/tmp/forepaw-\(tag).png"

        var resolvedWindow: ResolvedWindow?
        if let appName = app {
            let runningApp = try findApp(named: appName)
            runningApp.activate()
            try await Task.sleep(nanoseconds: 300_000_000)
            let resolved = try findWindow(pid: runningApp.processIdentifier, window: window)
            resolvedWindow = resolved
            var args = ["-x", "-o", "-l", String(resolved.windowID)]
            if options.cursor { args.insert("-C", at: 0) }
            args.append(rawPath)
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
            process.arguments = args
            try process.run()
            process.waitUntilExit()
        } else {
            var args = ["-x"]
            if options.cursor { args.insert("-C", at: 0) }
            args.append(rawPath)
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
            process.arguments = args
            try process.run()
            process.waitUntilExit()
        }

        guard let style = style, let appName = app else {
            let finalPath = try postProcessScreenshot(
                rawPath: rawPath, tag: tag, options: options)
            return ScreenshotResult(path: finalPath, annotations: nil, legend: nil)
        }

        // Get the AX tree for annotations
        let snapshotOpts = SnapshotOptions(
            interactiveOnly: true, maxDepth: SnapshotOptions.defaultDepth, compact: false)
        let tree = try await snapshot(app: appName, options: snapshotOpts)

        // Determine window bounds for coordinate conversion
        let windowBounds: Rect
        if let resolved = resolvedWindow {
            windowBounds = Rect(
                x: resolved.origin.x,
                y: resolved.origin.y,
                width: resolved.size.width,
                height: resolved.size.height
            )
        } else {
            // Full screen fallback
            let screen = NSScreen.main!
            windowBounds = Rect(x: 0, y: 0, width: screen.frame.width, height: screen.frame.height)
        }

        // Collect annotations
        let collector = AnnotationCollector()
        var annotations = collector.collect(from: tree, windowBounds: windowBounds)

        // Filter to specific refs if requested
        if let only = only, !only.isEmpty {
            let refSet = Set(only)
            annotations = annotations.filter { refSet.contains($0.ref) }
        }

        guard !annotations.isEmpty else {
            return ScreenshotResult(path: rawPath, annotations: nil, legend: "No interactive elements found")
        }

        // Render annotated image
        let annotatedPath = "/tmp/forepaw-\(tag)-annotated.png"
        let scaleFactor = NSScreen.main?.backingScaleFactor ?? 2.0
        let renderer = AnnotationRenderer()
        try renderer.render(
            imagePath: rawPath,
            annotations: annotations,
            style: style,
            scaleFactor: scaleFactor,
            outputPath: annotatedPath
        )

        // Generate legend
        let legend = AnnotationLegend().format(annotations: annotations)

        // Post-process the annotated image (scale + format conversion)
        let finalPath = try postProcessScreenshot(
            rawPath: annotatedPath, tag: tag, options: options,
            suffix: "-annotated")
        return ScreenshotResult(path: finalPath, annotations: annotations, legend: legend)
    }

    /// Validate that a point falls within the app's window bounds.
    /// Throws if outside -- a misplaced click could hit the wrong element or a different app.
    internal func validatePointInWindow(_ point: Point, pid: Int32) throws {
        guard let resolved = try? findWindow(pid: pid, window: nil) else {
            return  // Can't validate without window info -- allow the action
        }
        let bounds = Rect(
            x: resolved.origin.x, y: resolved.origin.y,
            width: resolved.size.width, height: resolved.size.height
        )
        if let error = CoordinateValidation.validate(point: point, bounds: bounds) {
            throw ForepawError.actionFailed(error)
        }
    }

    /// Post-process a screenshot: downscale to 1x and/or convert to JPEG.
    /// Returns the final output path (may be the same as input if no conversion needed).
    private func postProcessScreenshot(
        rawPath: String, tag: String, options: ScreenshotOptions,
        suffix: String = ""
    ) throws -> String {
        let needsScale = options.scale == 1
        let needsFormat = options.format == .jpeg

        guard needsScale || needsFormat else {
            return rawPath
        }

        let ext = options.format == .jpeg ? "jpg" : "png"
        let outputPath = "/tmp/forepaw-\(tag)\(suffix).\(ext)"

        var sipsArgs: [String] = []

        // Scale down to 1x (half the Retina 2x dimensions)
        if needsScale {
            guard let image = NSImage(contentsOfFile: rawPath),
                let rep = image.representations.first
            else {
                return rawPath
            }
            let targetWidth = rep.pixelsWide / 2
            sipsArgs += ["--resampleWidth", String(targetWidth)]
        }

        // Convert format
        if needsFormat {
            sipsArgs += ["-s", "format", "jpeg", "-s", "formatOptions", String(options.quality)]
        }

        sipsArgs += [rawPath, "--out", outputPath]

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/sips")
        process.arguments = sipsArgs
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()

        // Clean up intermediate PNG if we converted to a different file
        if outputPath != rawPath {
            try? FileManager.default.removeItem(atPath: rawPath)
        }

        return outputPath
    }
}
