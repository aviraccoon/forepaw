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

    internal func performMouseClick(
        at point: CGPoint, button: CGMouseButton = .left, clickCount: Int64 = 1
    ) throws {
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
}
