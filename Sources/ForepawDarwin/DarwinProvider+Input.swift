import ApplicationServices
import Cocoa
import ForepawCore

// MARK: - Input simulation (click, type, press, scroll)

extension DarwinProvider {
    public func click(ref: ElementRef) async throws -> ActionResult {
        guard let element = refTable[ref] else {
            throw ForepawError.staleRef(ref)
        }
        return try clickElement(element)
    }

    /// Click with an app name, resolving the ref by re-walking the tree.
    public func click(
        ref: ElementRef, app: String, options: ClickOptions = .normal
    ) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        let element = try resolveRef(ref, app: app)
        // Activate the app before mouse clicks -- CGEvent targets whatever
        // is under the cursor, so the app must be frontmost.
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)  // 300ms for activation
        return try clickElement(element, options: options)
    }

    internal func clickElement(
        _ element: AXUIElement, options: ClickOptions = .normal
    ) throws -> ActionResult {
        let role = getAttribute(element, kAXRoleAttribute) as? String ?? ""
        let button: CGMouseButton = options.button == .right ? .right : .left
        let isRightClick = options.button == .right
        let isDoubleClick = options.clickCount > 1

        // For web content links, prefer mouse click -- AXPress often doesn't
        // trigger navigation in browsers.
        // For right-click/double-click, always use mouse (AXPress can't do these).
        let preferMouse = role == "AXLink" || isRightClick || isDoubleClick

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
            let label = isRightClick ? "right-clicked" : isDoubleClick ? "double-clicked" : "clicked"
            try performMouseClick(at: point, button: button, clickCount: Int64(options.clickCount))
            return ActionResult(success: true, message: "\(label) at \(Int(point.x)),\(Int(point.y))")
        }

        // Last resort for links: try AXPress anyway (only for regular left click)
        if !isRightClick && !isDoubleClick && preferMouse {
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

    internal func setValueOnElement(_ element: AXUIElement, value: String) throws -> ActionResult {

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

    /// Scroll at a screen point using CGEvent scroll wheel events.
    ///
    /// - Parameters:
    ///   - point: Screen-space point to position the mouse before scrolling.
    ///   - deltaY: Vertical scroll amount in "lines". Positive = up, negative = down.
    ///   - deltaX: Horizontal scroll amount. Positive = left, negative = right.
    public func scroll(at point: CGPoint, deltaY: Int32, deltaX: Int32 = 0) throws {
        moveMouseToScrollTarget(point)
        try postScrollEvent(deltaY: deltaY, deltaX: deltaX)
    }

    /// Move the mouse to the scroll target point and wait for hover effects to settle.
    internal func moveMouseToScrollTarget(_ point: CGPoint) {
        guard
            let moveEvent = CGEvent(
                mouseEventSource: nil, mouseType: .mouseMoved,
                mouseCursorPosition: point, mouseButton: .left)
        else { return }
        moveEvent.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.05)
    }

    /// Post a scroll wheel event at the current mouse position.
    internal func postScrollEvent(deltaY: Int32, deltaX: Int32 = 0) throws {
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

        let resolved = try findWindow(pid: runningApp.processIdentifier, window: window)
        let scrollPoint: CGPoint

        if let ref = ref {
            // Scroll at the center of the referenced element
            let element = try resolveRef(ref, app: app)
            guard let pos = getPosition(of: element), let size = getSize(of: element) else {
                throw ForepawError.actionFailed("Cannot determine position of \(ref)")
            }
            scrollPoint = CGPoint(x: pos.x + size.width / 2, y: pos.y + size.height / 2)
        } else {
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

        // Move mouse to scroll target first, then capture fingerprint.
        // This ensures hover effects (e.g. Discord message highlight) are present
        // in both before and after captures.
        moveMouseToScrollTarget(scrollPoint)

        // Wait for hover effects to settle after mouse move
        try await Task.sleep(nanoseconds: 150_000_000)

        // Capture content fingerprint before scrolling for boundary detection
        let beforeFingerprint = captureScrollFingerprint(windowID: resolved.windowID)

        try postScrollEvent(deltaY: deltaY, deltaX: deltaX)

        // Wait for scroll to take effect before comparing.
        try await Task.sleep(nanoseconds: 150_000_000)

        // Detect boundary: compare content before and after
        let atBoundary: Bool
        if let before = beforeFingerprint,
            let after = captureScrollFingerprint(windowID: resolved.windowID)
        {
            atBoundary = before == after
        } else {
            atBoundary = false  // Can't determine -- assume not at boundary
        }

        let boundaryNote = atBoundary ? " (at boundary -- content did not change)" : ""
        return ActionResult(
            success: true,
            message:
                "scrolled \(direction) \(amount) ticks at \(Int(scrollPoint.x)),\(Int(scrollPoint.y))\(boundaryNote)"
        )
    }

    /// Capture a fingerprint of the window's content for scroll boundary detection.
    /// Returns raw pixel data of a thin horizontal strip from the center of the window.
    /// Fast: uses in-memory CGWindowListCreateImage, no file I/O.
    ///
    /// Uses CGWindowListCreateImage (deprecated macOS 14, to be replaced by ScreenCaptureKit).
    /// SCK is async and requires setup -- overkill for a synchronous pixel comparison.
    /// Migrate when Apple removes the API or we bump the deployment target.
    private func captureScrollFingerprint(windowID: CGWindowID) -> Data? {
        guard
            let image = CGWindowListCreateImage(
                .null, .optionIncludingWindow, windowID,
                [.boundsIgnoreFraming, .nominalResolution])
        else { return nil }

        let h = image.height
        let w = image.width
        guard h > 40 && w > 0 else { return nil }

        // Crop a 20px tall strip from the vertical center of the window.
        // This avoids title bars, toolbars, and status bars which don't change on scroll.
        // Exclude the rightmost 30px to avoid transient scrollbar overlays
        // (Electron apps like Discord show a fade-in/fade-out scrollbar on scroll).
        let stripY = h / 2 - 10
        let stripW = max(1, w - 30)
        let stripRect = CGRect(x: 0, y: stripY, width: stripW, height: 20)
        guard let strip = image.cropping(to: stripRect),
            let provider = strip.dataProvider,
            let data = provider.data
        else { return nil }
        return data as Data
    }

    internal func performMouseClick(
        at point: CGPoint, button: CGMouseButton = .left, clickCount: Int64 = 1
    ) throws {
        // Move the physical cursor to the click target first.
        // CGEvent mouseDown at a position doesn't always route to the right window
        // unless the cursor is actually there.
        guard
            let moveEvent = CGEvent(
                mouseEventSource: nil, mouseType: .mouseMoved,
                mouseCursorPosition: point, mouseButton: .left)
        else {
            throw ForepawError.actionFailed("Failed to create mouse move event")
        }
        moveEvent.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.05)

        let downType: CGEventType = button == .right ? .rightMouseDown : .leftMouseDown
        let upType: CGEventType = button == .right ? .rightMouseUp : .leftMouseUp

        for i in 1...clickCount {
            guard
                let mouseDown = CGEvent(
                    mouseEventSource: nil, mouseType: downType, mouseCursorPosition: point, mouseButton: button),
                let mouseUp = CGEvent(
                    mouseEventSource: nil, mouseType: upType, mouseCursorPosition: point, mouseButton: button)
            else {
                throw ForepawError.actionFailed("Failed to create mouse events")
            }
            mouseDown.setIntegerValueField(.mouseEventClickState, value: i)
            mouseUp.setIntegerValueField(.mouseEventClickState, value: i)
            mouseDown.post(tap: .cghidEventTap)
            mouseUp.post(tap: .cghidEventTap)
            if i < clickCount {
                Thread.sleep(forTimeInterval: 0.01)  // small delay between clicks
            }
        }
    }

    internal func typeViaKeyboard(_ text: String) throws {
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

    internal func pressViaKeyboard(_ combo: KeyCombo) throws {
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

    /// Press with app activation -- ensures keystrokes go to the right app.
    public func press(keys: KeyCombo, app: String) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        try pressViaKeyboard(keys)
        return ActionResult(success: true)
    }

    /// Resolve a ref to its center point in screen coordinates.
    public func resolveRefPosition(_ ref: ElementRef, app: String) throws -> Point {
        let element = try resolveRef(ref, app: app)
        guard let pos = getPosition(of: element), let size = getSize(of: element) else {
            throw ForepawError.actionFailed("Cannot determine position of \(ref)")
        }
        return Point(x: pos.x + size.width / 2, y: pos.y + size.height / 2)
    }

    /// Resolve a ref to its bounding rectangle in screen coordinates.
    public func resolveRefBounds(_ ref: ElementRef, app: String) throws -> Rect {
        let element = try resolveRef(ref, app: app)
        guard let pos = getPosition(of: element), let size = getSize(of: element) else {
            throw ForepawError.actionFailed("Cannot determine bounds of \(ref)")
        }
        return Rect(x: pos.x, y: pos.y, width: size.width, height: size.height)
    }

    /// Move the mouse to a screen point without clicking.
    public func moveMouse(to point: CGPoint) throws {
        guard
            let moveEvent = CGEvent(
                mouseEventSource: nil, mouseType: .mouseMoved,
                mouseCursorPosition: point, mouseButton: .left)
        else {
            throw ForepawError.actionFailed("Failed to create mouse move event")
        }
        moveEvent.post(tap: .cghidEventTap)
    }

    /// Move the mouse to a screen point, optionally activating an app first.
    public func hoverAtPoint(_ point: Point, app: String? = nil) async throws -> ActionResult {
        if let app {
            let runningApp = try findApp(named: app)
            runningApp.activate()
            try await Task.sleep(nanoseconds: 300_000_000)
            try validatePointInWindow(point, pid: runningApp.processIdentifier)
        }
        try moveMouse(to: CGPoint(x: point.x, y: point.y))
        return ActionResult(success: true, message: "hovered at \(Int(point.x)),\(Int(point.y))")
    }

    /// Move the mouse to an element's center without clicking.
    /// Triggers tooltips, hover states, dropdown previews.
    public func hover(ref: ElementRef, app: String) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        let element = try resolveRef(ref, app: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)

        guard let pos = getPosition(of: element), let size = getSize(of: element) else {
            throw ForepawError.actionFailed("Cannot determine position of \(ref)")
        }
        let point = CGPoint(x: pos.x + size.width / 2, y: pos.y + size.height / 2)
        try moveMouse(to: point)

        return ActionResult(success: true, message: "hovered at \(Int(point.x)),\(Int(point.y))")
    }

    /// Move the mouse to text found via OCR without clicking.
    public func ocrHover(
        text: String, app: String, window: String? = nil, index: Int? = nil
    ) async throws -> ActionResult {
        let match = try await resolveOCRText(text, app: app, window: window, index: index)

        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)
        try moveMouse(to: match.point)

        return ActionResult(
            success: true,
            message: "hovered '\(match.text)' at \(Int(match.point.x)),\(Int(match.point.y))")
    }

    /// Drag along a path of screen coordinates.
    ///
    /// For two-point drag, pass a 2-element path.
    /// For multi-point path, pass 3+ points.
    /// If `options.closePath` is true, the first point is appended to close the shape.
    public func drag(
        path: [Point], options: DragOptions = DragOptions(),
        app: String? = nil
    ) async throws -> ActionResult {
        var cgPath = path.map { CGPoint(x: $0.x, y: $0.y) }
        guard cgPath.count >= 2 else {
            throw ForepawError.actionFailed("Drag path requires at least 2 points")
        }

        if let app {
            let runningApp = try findApp(named: app)
            runningApp.activate()
            try await Task.sleep(nanoseconds: 300_000_000)
        }

        if options.closePath, cgPath.count >= 3, let first = cgPath.first {
            cgPath.append(first)
        }

        try performMouseDrag(path: cgPath, options: options)

        if cgPath.count == 2 {
            return ActionResult(
                success: true,
                message:
                    "dragged from \(Int(cgPath[0].x)),\(Int(cgPath[0].y)) to \(Int(cgPath[1].x)),\(Int(cgPath[1].y))"
                    + " (\(options.steps) steps, \(String(format: "%.1f", options.duration))s)")
        }
        return ActionResult(
            success: true,
            message:
                "dragged through \(cgPath.count) points"
                + " (\(options.steps) steps/segment, \(String(format: "%.1f", options.duration))s)")
    }

    /// Drag from one element to another.
    public func drag(
        fromRef: ElementRef, toRef: ElementRef, app: String,
        options: DragOptions = DragOptions()
    ) async throws -> ActionResult {
        let runningApp = try findApp(named: app)
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)

        let from = try resolveRefPosition(fromRef, app: app)
        let to = try resolveRefPosition(toRef, app: app)

        try performMouseDrag(
            path: [CGPoint(x: from.x, y: from.y), CGPoint(x: to.x, y: to.y)], options: options)
        return ActionResult(
            success: true,
            message:
                "dragged from \(Int(from.x)),\(Int(from.y)) to \(Int(to.x)),\(Int(to.y))"
                + " (\(options.steps) steps, \(String(format: "%.1f", options.duration))s)")
    }

    // MARK: - Drag internals

    /// Convert KeyCombo.Modifier array to CGEventFlags.
    private func eventFlags(for modifiers: [KeyCombo.Modifier]) -> CGEventFlags {
        var flags = CGEventFlags()
        for modifier in modifiers {
            switch modifier {
            case .command: flags.insert(.maskCommand)
            case .shift: flags.insert(.maskShift)
            case .option: flags.insert(.maskAlternate)
            case .control: flags.insert(.maskControl)
            }
        }
        return flags
    }

    /// Apply drag options (modifiers, pressure) to a CGEvent.
    private func applyDragOptions(_ event: CGEvent, options: DragOptions) {
        if !options.modifiers.isEmpty {
            event.flags = eventFlags(for: options.modifiers)
        }
        if let pressure = options.pressure {
            event.setDoubleValueField(.mouseEventPressure, value: pressure)
        }
    }

    /// Unified drag implementation for any path length.
    internal func performMouseDrag(
        path: [CGPoint], options: DragOptions
    ) throws {
        guard let first = path.first, let last = path.last else { return }

        let button: CGMouseButton = options.rightButton ? .right : .left
        let downType: CGEventType = options.rightButton ? .rightMouseDown : .leftMouseDown
        let dragType: CGEventType = options.rightButton ? .rightMouseDragged : .leftMouseDragged
        let upType: CGEventType = options.rightButton ? .rightMouseUp : .leftMouseUp

        // Move to start position
        try moveMouse(to: first)
        Thread.sleep(forTimeInterval: 0.05)

        // Mouse down
        guard
            let mouseDown = CGEvent(
                mouseEventSource: nil, mouseType: downType,
                mouseCursorPosition: first, mouseButton: button)
        else {
            throw ForepawError.actionFailed("Failed to create mouseDown event")
        }
        applyDragOptions(mouseDown, options: options)
        mouseDown.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.02)

        // Drag through segments
        let segments = path.count - 1
        let segmentDuration = options.duration / Double(segments)
        let stepDelay = segmentDuration / Double(options.steps)

        for segIdx in 0..<segments {
            let segFrom = path[segIdx]
            let segTo = path[segIdx + 1]
            for i in 1...options.steps {
                let t = Double(i) / Double(options.steps)
                let x = segFrom.x + (segTo.x - segFrom.x) * t
                let y = segFrom.y + (segTo.y - segFrom.y) * t
                let point = CGPoint(x: x, y: y)

                guard
                    let dragEvent = CGEvent(
                        mouseEventSource: nil, mouseType: dragType,
                        mouseCursorPosition: point, mouseButton: button)
                else { continue }
                applyDragOptions(dragEvent, options: options)
                dragEvent.post(tap: .cghidEventTap)
                Thread.sleep(forTimeInterval: stepDelay)
            }
        }

        // Mouse up
        guard
            let mouseUp = CGEvent(
                mouseEventSource: nil, mouseType: upType,
                mouseCursorPosition: last, mouseButton: button)
        else {
            throw ForepawError.actionFailed("Failed to create mouseUp event")
        }
        applyDragOptions(mouseUp, options: options)
        mouseUp.post(tap: .cghidEventTap)
    }

    /// Wait for text to appear on screen via OCR polling.
    ///
    /// - Parameters:
    ///   - text: Text to search for (case-insensitive substring)
    ///   - app: Target application name
    ///   - window: Optional window title or ID
    ///   - timeout: Maximum seconds to wait (default 10)
    ///   - interval: Seconds between polls (default 1)
    /// - Returns: ActionResult with the matched text
    public func wait(
        text: String, app: String, window: String? = nil,
        timeout: Double = 10, interval: Double = 1
    ) async throws -> ActionResult {
        let deadline = Date().addingTimeInterval(timeout)

        while Date() < deadline {
            let matches = try await ocr(app: app, window: window, find: text, screenshotOptions: nil).results
            if let match = matches.first {
                return ActionResult(success: true, message: "found '\(match.text)' after waiting")
            }
            let remaining = deadline.timeIntervalSinceNow
            if remaining <= 0 { break }
            let sleepTime = min(interval, remaining)
            try await Task.sleep(nanoseconds: UInt64(sleepTime * 1_000_000_000))
        }

        throw ForepawError.actionFailed("Timed out after \(Int(timeout))s waiting for '\(text)'")
    }
}
