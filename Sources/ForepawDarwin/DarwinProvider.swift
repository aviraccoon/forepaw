import ApplicationServices
import Cocoa
import ForepawCore

/// A resolved CGWindowList window with its ID, title, and bounds.
public struct ResolvedWindow: Sendable {
    public let windowID: CGWindowID
    public let title: String
    public let bounds: [String: Double]

    public var origin: CGPoint {
        CGPoint(x: bounds["X"] ?? 0, y: bounds["Y"] ?? 0)
    }

    public var size: CGSize {
        CGSize(width: bounds["Width"] ?? 0, height: bounds["Height"] ?? 0)
    }

    public var center: CGPoint {
        CGPoint(x: origin.x + size.width / 2, y: origin.y + size.height / 2)
    }
}

/// macOS implementation of `DesktopProvider` using Accessibility APIs.
public final class DarwinProvider: DesktopProvider, @unchecked Sendable {
    // Current snapshot's ref table, keyed by ref ID.
    // Stores AXUIElement handles for action dispatch.
    private var refTable: [ElementRef: AXUIElement] = [:]

    public init() {}

    // MARK: - Permissions

    public func hasPermissions() -> Bool {
        AXIsProcessTrusted()
    }

    public func hasScreenRecordingPermission() -> Bool {
        CGPreflightScreenCaptureAccess()
    }

    public func requestPermissions() -> Bool {
        let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
        return AXIsProcessTrustedWithOptions(options)
    }

    public func requestScreenRecordingPermission() -> Bool {
        CGRequestScreenCaptureAccess()
    }

    // MARK: - DesktopProvider

    public func listApps() async throws -> [AppInfo] {
        NSWorkspace.shared.runningApplications
            .filter { $0.activationPolicy == .regular }
            .compactMap { app in
                guard let name = app.localizedName else { return nil }
                return AppInfo(
                    name: name,
                    bundleID: app.bundleIdentifier,
                    pid: app.processIdentifier
                )
            }
    }

    public func snapshot(app appName: String, options: SnapshotOptions) async throws -> ElementTree {
        guard AXIsProcessTrusted() else {
            throw ForepawError.permissionDenied
        }
        let runningApp = try findApp(named: appName)
        let appElement = AXUIElementCreateApplication(runningApp.processIdentifier)

        let root = buildTree(element: appElement, depth: 0, maxDepth: options.maxDepth)

        let assigner = RefAssigner()
        let result = assigner.assign(root: root, interactiveOnly: options.interactiveOnly)

        // Build platform ref table for action dispatch
        refTable.removeAll()
        var axElements: [Int: AXUIElement] = [:]
        var axCounter = 1
        collectAXElements(
            element: appElement, depth: 0, maxDepth: options.maxDepth, counter: &axCounter, elements: &axElements)

        for (ref, _) in result.refs {
            if let axElement = axElements[ref.id] {
                refTable[ref] = axElement
            }
        }

        return ElementTree(app: appName, root: result.root, refs: result.refs)
    }

    /// Re-walk the tree to find the AXUIElement for a given ref.
    /// Refs are positional (Nth interactive element in depth-first order),
    /// so this works across process invocations as long as the UI hasn't changed.
    public func resolveRef(_ ref: ElementRef, app appName: String) throws -> AXUIElement {
        guard AXIsProcessTrusted() else { throw ForepawError.permissionDenied }
        let runningApp = try findApp(named: appName)
        let appElement = AXUIElementCreateApplication(runningApp.processIdentifier)
        var elements: [Int: AXUIElement] = [:]
        var resolveCounter = 1
        collectAXElements(element: appElement, depth: 0, maxDepth: 15, counter: &resolveCounter, elements: &elements)
        guard let element = elements[ref.id] else {
            throw ForepawError.staleRef(ref)
        }
        return element
    }

    public func click(ref: ElementRef) async throws -> ActionResult {
        guard let element = refTable[ref] else {
            throw ForepawError.staleRef(ref)
        }
        return try clickElement(element)
    }

    /// Click with an app name, resolving the ref by re-walking the tree.
    public func click(ref: ElementRef, app: String) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        let element = try resolveRef(ref, app: app)
        // Activate the app before mouse clicks -- CGEvent targets whatever
        // is under the cursor, so the app must be frontmost.
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)  // 300ms for activation
        return try clickElement(element)
    }

    private func clickElement(_ element: AXUIElement) throws -> ActionResult {
        let role = getAttribute(element, kAXRoleAttribute) as? String ?? ""

        // For web content links, prefer mouse click -- AXPress often doesn't
        // trigger navigation in browsers.
        let preferMouse = role == "AXLink"

        if !preferMouse {
            // Try AXPress first (accessibility action)
            let pressResult = AXUIElementPerformAction(element, kAXPressAction as CFString)
            if pressResult == .success {
                return ActionResult(success: true, message: "pressed via AX")
            }
        }

        // Mouse click at element center
        if let position = getPosition(of: element), let size = getSize(of: element) {
            let point = CGPoint(x: position.x + size.width / 2, y: position.y + size.height / 2)
            try performMouseClick(at: point)
            return ActionResult(success: true, message: "clicked at \(Int(point.x)),\(Int(point.y))")
        }

        // Last resort for links: try AXPress anyway
        if preferMouse {
            let pressResult = AXUIElementPerformAction(element, kAXPressAction as CFString)
            if pressResult == .success {
                return ActionResult(success: true, message: "pressed via AX (fallback)")
            }
        }

        return ActionResult(success: false, message: "click failed: no position and AXPress unsuccessful")
    }

    public func type(ref: ElementRef, text: String) async throws -> ActionResult {
        try await setValue(ref: ref, value: text)
    }

    /// Type with an app name, resolving the ref by re-walking the tree.
    public func type(ref: ElementRef, text: String, app: String) async throws -> ActionResult {
        try await setValue(ref: ref, value: text, app: app)
    }

    public func setValue(ref: ElementRef, value: String) async throws -> ActionResult {
        guard let element = refTable[ref] else {
            throw ForepawError.staleRef(ref)
        }
        return try setValueOnElement(element, value: value)
    }

    /// Set value with an app name, resolving the ref by re-walking the tree.
    public func setValue(ref: ElementRef, value: String, app: String) async throws -> ActionResult {
        let element = try resolveRef(ref, app: app)
        return try setValueOnElement(element, value: value)
    }

    private func setValueOnElement(_ element: AXUIElement, value: String) throws -> ActionResult {

        let result = AXUIElementSetAttributeValue(element, kAXValueAttribute as CFString, value as CFTypeRef)
        if result == .success {
            return ActionResult(success: true)
        }

        // Fallback: focus and type via CGEvent
        AXUIElementPerformAction(element, kAXRaiseAction as CFString)
        AXUIElementSetAttributeValue(element, kAXFocusedAttribute as CFString, true as CFTypeRef)
        try typeViaKeyboard(value)
        return ActionResult(success: true, message: "typed via keyboard simulation")
    }

    public func press(keys: KeyCombo) async throws -> ActionResult {
        try pressViaKeyboard(keys)
        return ActionResult(success: true)
    }

    /// Type text into whatever is currently focused (no app activation).
    public func keyboardType(text: String) async throws -> ActionResult {
        try typeViaKeyboard(text)
        return ActionResult(success: true, message: "typed \(text.count) chars")
    }

    /// Type text into whatever is currently focused in the target app.
    public func keyboardType(text: String, app: String) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        try typeViaKeyboard(text)
        return ActionResult(success: true, message: "typed \(text.count) chars")
    }

    /// Screenshot + OCR, returning recognized text with screen coordinates.
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
    public func ocrClick(text: String, app: String, window: String? = nil) async throws -> ActionResult {
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

        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        try performMouseClick(at: screenPoint)
        return ActionResult(
            success: true, message: "clicked '\(match.text)' at \(Int(screenPoint.x)),\(Int(screenPoint.y))")
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
    /// - Throws: `ForepawError.windowNotFound` if no matching window exists
    public func findWindow(pid: Int32, window: String? = nil) throws -> ResolvedWindow {
        let windowList = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []

        // Collect all real windows for this app (skip phantoms)
        var appWindows: [(id: CGWindowID, title: String, bounds: [String: Double])] = []
        for info in windowList {
            guard let ownerPID = info[kCGWindowOwnerPID as String] as? Int32,
                ownerPID == pid,
                let bounds = info[kCGWindowBounds as String] as? [String: Double]
            else { continue }
            let w = bounds["Width"] ?? 0
            let h = bounds["Height"] ?? 0
            guard w >= 10 && h >= 10 else { continue }
            let windowID = (info[kCGWindowNumber as String] as? Int).map { CGWindowID($0) } ?? 0
            let title = info[kCGWindowName as String] as? String ?? ""
            appWindows.append((id: windowID, title: title, bounds: bounds))
        }

        guard !appWindows.isEmpty else {
            throw ForepawError.windowNotFound(window ?? "any")
        }

        if let window = window {
            // Match by window ID: "w-1234"
            if window.hasPrefix("w-"), let idNum = UInt32(window.dropFirst(2)) {
                if let match = appWindows.first(where: { $0.id == CGWindowID(idNum) }) {
                    return ResolvedWindow(windowID: match.id, title: match.title, bounds: match.bounds)
                }
                throw ForepawError.windowNotFound(window)
            }

            // Substring match on title (case-insensitive)
            let matches = appWindows.filter {
                $0.title.localizedCaseInsensitiveContains(window)
            }
            if matches.count == 1 {
                let m = matches[0]
                return ResolvedWindow(windowID: m.id, title: m.title, bounds: m.bounds)
            }
            if matches.count > 1 {
                let titles = matches.map { "  w-\($0.id)  \($0.title)" }.joined(separator: "\n")
                throw ForepawError.ambiguousWindow(window, titles)
            }
            throw ForepawError.windowNotFound(window)
        }

        // Default: largest window by area
        let best = appWindows.max(by: {
            let a1 = ($0.bounds["Width"] ?? 0) * ($0.bounds["Height"] ?? 0)
            let a2 = ($1.bounds["Width"] ?? 0) * ($1.bounds["Height"] ?? 0)
            return a1 < a2
        })!
        return ResolvedWindow(windowID: best.id, title: best.title, bounds: best.bounds)
    }

    /// Press with app activation -- ensures keystrokes go to the right app.
    public func press(keys: KeyCombo, app: String) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        try pressViaKeyboard(keys)
        return ActionResult(success: true)
    }

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

    public func listWindows(app appName: String?) async throws -> [WindowInfo] {
        let windowList = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []

        return windowList.compactMap { info -> WindowInfo? in
            guard let name = info[kCGWindowOwnerName as String] as? String,
                let windowID = info[kCGWindowNumber as String] as? Int,
                let title = info[kCGWindowName as String] as? String
            else { return nil }

            if let filter = appName, name != filter { return nil }

            // Skip phantom/tiny windows
            if let boundsDict = info[kCGWindowBounds as String] as? [String: Double] {
                let w = boundsDict["Width"] ?? 0
                let h = boundsDict["Height"] ?? 0
                if w < 10 || h < 10 { return nil }
            }

            var bounds: Rect?
            if let boundsDict = info[kCGWindowBounds as String] as? [String: Double] {
                bounds = Rect(
                    x: boundsDict["X"] ?? 0,
                    y: boundsDict["Y"] ?? 0,
                    width: boundsDict["Width"] ?? 0,
                    height: boundsDict["Height"] ?? 0
                )
            }

            return WindowInfo(id: "w-\(windowID)", title: title, app: name, bounds: bounds)
        }
    }

    // MARK: - Private helpers

    private func findApp(named name: String) throws -> NSRunningApplication {
        let apps = NSWorkspace.shared.runningApplications.filter { $0.activationPolicy == .regular }
        if let app = apps.first(where: { $0.localizedName == name }) {
            return app
        }
        if let app = apps.first(where: { $0.bundleIdentifier == name }) {
            return app
        }
        // Case-insensitive partial match
        if let app = apps.first(where: { $0.localizedName?.localizedCaseInsensitiveContains(name) == true }) {
            return app
        }
        throw ForepawError.appNotFound(name)
    }

    private func buildTree(element: AXUIElement, depth: Int, maxDepth: Int) -> ElementNode {
        guard depth < maxDepth else { return ElementNode(role: "AXGroup") }

        let role = getAttribute(element, kAXRoleAttribute) as? String ?? "AXUnknown"
        let name =
            getAttribute(element, kAXTitleAttribute) as? String
            ?? getAttribute(element, kAXDescriptionAttribute) as? String
            ?? computedName(of: element)
        let value = getAttribute(element, kAXValueAttribute).flatMap { val -> String? in
            if let s = val as? String { return s }
            if let n = val as? NSNumber { return n.stringValue }
            return nil
        }

        var bounds: Rect?
        if let pos = getPosition(of: element), let size = getSize(of: element) {
            bounds = Rect(x: pos.x, y: pos.y, width: size.width, height: size.height)
        }

        var children: [ElementNode] = []
        if let childrenRef = getAttribute(element, kAXChildrenAttribute) as? [AXUIElement] {
            children = childrenRef.map { buildTree(element: $0, depth: depth + 1, maxDepth: maxDepth) }
        }

        return ElementNode(
            role: role,
            name: name,
            value: value,
            bounds: bounds,
            children: children
        )
    }

    /// Walk the AX tree to map ref positions to AXUIElement handles.
    /// Must mirror the depth-first order used by RefAssigner.
    private func collectAXElements(
        element: AXUIElement,
        depth: Int,
        maxDepth: Int,
        counter: inout Int,
        elements: inout [Int: AXUIElement]
    ) {
        guard depth < maxDepth else { return }

        let role = getAttribute(element, kAXRoleAttribute) as? String ?? "AXUnknown"
        if ElementNode.isInteractiveRole(role) {
            elements[counter] = element
            counter += 1
        }

        if let children = getAttribute(element, kAXChildrenAttribute) as? [AXUIElement] {
            for child in children {
                collectAXElements(
                    element: child, depth: depth + 1, maxDepth: maxDepth, counter: &counter, elements: &elements)
            }
        }
    }

    /// Derive a name from AXTitleUIElement or first AXStaticText child.
    /// Many elements (cells, rows) don't have a direct title but point
    /// to a label element via AXTitleUIElement, or contain a static text child.
    private func computedName(of element: AXUIElement) -> String? {
        // 1. AXTitleUIElement -> read its value or title
        if let titleElement = getAttribute(element, kAXTitleUIElementAttribute) as! AXUIElement? {
            if let val = getAttribute(titleElement, kAXValueAttribute) as? String, !val.isEmpty {
                return val
            }
            if let title = getAttribute(titleElement, kAXTitleAttribute) as? String, !title.isEmpty {
                return title
            }
        }

        // 2. First AXStaticText child's value
        if let children = getAttribute(element, kAXChildrenAttribute) as? [AXUIElement] {
            for child in children {
                let childRole = getAttribute(child, kAXRoleAttribute) as? String
                if childRole == "AXStaticText" {
                    if let val = getAttribute(child, kAXValueAttribute) as? String, !val.isEmpty {
                        return val
                    }
                }
            }
        }

        return nil
    }

    private func getAttribute(_ element: AXUIElement, _ attribute: String) -> Any? {
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
        guard result == .success else { return nil }
        return value
    }

    private func getPosition(of element: AXUIElement) -> CGPoint? {
        guard let value = getAttribute(element, kAXPositionAttribute) else { return nil }
        var point = CGPoint.zero
        // swiftlint:disable:next force_cast
        AXValueGetValue(value as! AXValue, .cgPoint, &point)
        return point
    }

    private func getSize(of element: AXUIElement) -> CGSize? {
        guard let value = getAttribute(element, kAXSizeAttribute) else { return nil }
        var size = CGSize.zero
        // swiftlint:disable:next force_cast
        AXValueGetValue(value as! AXValue, .cgSize, &size)
        return size
    }

    /// Scroll at a screen point using CGEvent scroll wheel events.
    ///
    /// - Parameters:
    ///   - point: Screen-space point to position the mouse before scrolling.
    ///   - deltaY: Vertical scroll amount in "lines". Positive = up, negative = down.
    ///   - deltaX: Horizontal scroll amount. Positive = left, negative = right.
    public func scroll(at point: CGPoint, deltaY: Int32, deltaX: Int32 = 0) throws {
        // Move mouse to the scroll target so the scroll event hits the right element.
        guard
            let moveEvent = CGEvent(
                mouseEventSource: nil, mouseType: .mouseMoved,
                mouseCursorPosition: point, mouseButton: .left)
        else {
            throw ForepawError.actionFailed("Failed to create mouse move event")
        }
        moveEvent.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.05)  // let the move settle

        guard
            let scrollEvent = CGEvent(
                scrollWheelEvent2Source: nil, units: .line,
                wheelCount: 2, wheel1: deltaY, wheel2: deltaX, wheel3: 0)
        else {
            throw ForepawError.actionFailed("Failed to create scroll event")
        }
        scrollEvent.post(tap: .cghidEventTap)
    }

    /// Scroll within an app's main window.
    ///
    /// - Parameters:
    ///   - direction: "up", "down", "left", "right"
    ///   - amount: Number of scroll ticks (default 3)
    ///   - app: Target application name
    ///   - window: Optional window title or ID to target
    ///   - ref: Optional element ref to scroll within (scrolls at element center)
    public func scroll(
        direction: String, amount: Int = 3, app: String, window: String? = nil, ref: ElementRef? = nil
    ) async throws
        -> ActionResult
    {
        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)

        let scrollPoint: CGPoint

        if let ref = ref {
            // Scroll at the center of the referenced element
            let element = try resolveRef(ref, app: app)
            guard let pos = getPosition(of: element), let size = getSize(of: element) else {
                throw ForepawError.actionFailed("Cannot determine position of \(ref)")
            }
            scrollPoint = CGPoint(x: pos.x + size.width / 2, y: pos.y + size.height / 2)
        } else {
            // Scroll at the center of the targeted window
            let resolved = try findWindow(pid: runningApp.processIdentifier, window: window)
            scrollPoint = resolved.center
        }

        let deltaY: Int32
        let deltaX: Int32
        switch direction {
        case "up":
            deltaY = Int32(amount)
            deltaX = 0
        case "down":
            deltaY = -Int32(amount)
            deltaX = 0
        case "left":
            deltaY = 0
            deltaX = Int32(amount)
        case "right":
            deltaY = 0
            deltaX = -Int32(amount)
        default:
            throw ForepawError.actionFailed("Unknown direction '\(direction)'. Use up, down, left, or right.")
        }

        try scroll(at: scrollPoint, deltaY: deltaY, deltaX: deltaX)
        return ActionResult(
            success: true,
            message: "scrolled \(direction) \(amount) ticks at \(Int(scrollPoint.x)),\(Int(scrollPoint.y))")
    }

    private func performMouseClick(at point: CGPoint) throws {
        guard
            let mouseDown = CGEvent(
                mouseEventSource: nil, mouseType: .leftMouseDown, mouseCursorPosition: point, mouseButton: .left),
            let mouseUp = CGEvent(
                mouseEventSource: nil, mouseType: .leftMouseUp, mouseCursorPosition: point, mouseButton: .left)
        else {
            throw ForepawError.actionFailed("Failed to create mouse events")
        }
        mouseDown.post(tap: .cghidEventTap)
        mouseUp.post(tap: .cghidEventTap)
    }

    private func typeViaKeyboard(_ text: String) throws {
        for char in text {
            let string = String(char)
            guard let event = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true) else { continue }
            event.keyboardSetUnicodeString(stringLength: string.count, unicodeString: Array(string.utf16))
            event.post(tap: .cghidEventTap)

            guard let upEvent = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false) else { continue }
            upEvent.post(tap: .cghidEventTap)

            // Small delay between keystrokes -- Electron apps (Discord, Slack)
            // drop characters if events arrive too fast.
            Thread.sleep(forTimeInterval: 0.008)
        }
    }

    private func pressViaKeyboard(_ combo: KeyCombo) throws {
        let keyCode = KeyCodeMap.virtualKeyCode(for: combo.key) ?? 0
        var flags: CGEventFlags = []
        for modifier in combo.modifiers {
            switch modifier {
            case .command: flags.insert(.maskCommand)
            case .shift: flags.insert(.maskShift)
            case .option: flags.insert(.maskAlternate)
            case .control: flags.insert(.maskControl)
            }
        }

        guard let keyDown = CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(keyCode), keyDown: true),
            let keyUp = CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(keyCode), keyDown: false)
        else {
            throw ForepawError.actionFailed("Failed to create keyboard events")
        }

        keyDown.flags = flags
        keyUp.flags = flags
        keyDown.post(tap: .cghidEventTap)
        keyUp.post(tap: .cghidEventTap)
    }
}
