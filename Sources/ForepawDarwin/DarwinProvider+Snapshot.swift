import ApplicationServices
import Cocoa
import ForepawCore

// MARK: - Snapshot & AX tree

extension DarwinProvider {
    public func snapshot(app appName: String, options: SnapshotOptions) async throws -> ElementTree {
        guard AXIsProcessTrusted() else {
            throw ForepawError.permissionDenied
        }
        let runningApp = try findApp(named: appName)

        // Activate the app so the AX tree matches what action commands will see.
        // Some apps (e.g. browsers) only expose web content elements when active.
        runningApp.activate()
        try await Task.sleep(nanoseconds: 300_000_000)

        // Electron apps (Discord, Slack, VS Code, etc.) don't expose their web content
        // through the accessibility tree unless explicitly told to. Setting AXManualAccessibility
        // tells Chromium to build the full a11y tree, like VoiceOver would trigger.
        let isElectron = isElectronApp(runningApp)
        if isElectron {
            enableElectronAccessibility(runningApp)
            // Chromium needs time to build the tree after first enable.
            // Poll until web content appears (or timeout after ~3s).
            if !electronTreeIsPopulated(runningApp) {
                for _ in 0..<6 {
                    try await Task.sleep(nanoseconds: 500_000_000)
                    if electronTreeIsPopulated(runningApp) { break }
                }
            }
        }

        // Electron apps nest web content deeply (13+ levels of groups from DOM structure).
        // Use a higher depth to reach interactive elements inside the web area.
        let effectiveDepth =
            isElectron ? max(options.maxDepth, Self.electronDepth) : options.maxDepth

        let appElement = AXUIElementCreateApplication(runningApp.processIdentifier)

        let root = buildTree(element: appElement, depth: 0, maxDepth: effectiveDepth)

        let assigner = RefAssigner()
        let result = assigner.assign(root: root, interactiveOnly: options.interactiveOnly)

        // Build platform ref table for action dispatch
        refTable.removeAll()
        var axElements: [Int: AXUIElement] = [:]
        var axCounter = 1
        collectAXElements(
            element: appElement, depth: 0, maxDepth: effectiveDepth, counter: &axCounter,
            elements: &axElements)

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

        // Ensure Electron accessibility is enabled for action dispatch too.
        let isElectron = isElectronApp(runningApp)
        if isElectron {
            enableElectronAccessibility(runningApp)
        }

        let resolveDepth = isElectron ? Self.electronDepth : Self.defaultDepth
        let appElement = AXUIElementCreateApplication(runningApp.processIdentifier)
        var elements: [Int: AXUIElement] = [:]
        var resolveCounter = 1
        collectAXElements(
            element: appElement, depth: 0, maxDepth: resolveDepth, counter: &resolveCounter,
            elements: &elements)
        guard let element = elements[ref.id] else {
            throw ForepawError.staleRef(ref)
        }
        return element
    }

    internal func buildTree(element: AXUIElement, depth: Int, maxDepth: Int) -> ElementNode {
        guard depth < maxDepth else { return ElementNode(role: "AXGroup") }

        let role = getAttribute(element, kAXRoleAttribute) as? String ?? "AXUnknown"
        let name =
            nonEmpty(getAttribute(element, kAXTitleAttribute) as? String)
            ?? nonEmpty(getAttribute(element, kAXDescriptionAttribute) as? String)
            ?? computedName(of: element, role: role)
        let value = getAttribute(element, kAXValueAttribute).flatMap { val -> String? in
            if let s = val as? String { return s }
            if let n = val as? NSNumber { return n.stringValue }
            return nil
        }

        var bounds: Rect?
        if let pos = getPosition(of: element), let size = getSize(of: element) {
            bounds = Rect(x: pos.x, y: pos.y, width: size.width, height: size.height)
        }

        // Collect extra attributes that give agents useful context.
        var attributes: [String: String] = [:]

        if let subrole = getAttribute(element, kAXSubroleAttribute) as? String,
            !subrole.isEmpty, subrole != "AXNone"
        {
            attributes["subrole"] = subrole
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
            attributes: attributes,
            children: children
        )
    }

    /// Walk the AX tree to map ref positions to AXUIElement handles.
    /// Must mirror the depth-first order used by RefAssigner.
    internal func collectAXElements(
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

    /// Derive a name from multiple fallback sources.
    ///
    /// The chain (in priority order):
    /// 1. AXTitleUIElement -> its value or title
    /// 2. First AXStaticText child's value
    /// 3. AXHelp (descriptive help text)
    /// 4. AXPlaceholderValue (text field placeholder)
    /// 5. AXDOMClassList -> icon class parsing (Electron apps with Lucide, Tabler, etc.)
    /// 6. AXRoleDescription (when more specific than the generic role name)
    internal func computedName(of element: AXUIElement, role: String = "") -> String? {
        // 1. AXTitleUIElement -> read its value or title
        if let titleElement = getAttribute(element, kAXTitleUIElementAttribute) as! AXUIElement? {
            if let val = getAttribute(titleElement, kAXValueAttribute) as? String, !val.isEmpty {
                return val
            }
            if let title = getAttribute(titleElement, kAXTitleAttribute) as? String, !title.isEmpty {
                return title
            }
        }

        // 2. First AXStaticText child's value, or AXImage child with icon classes.
        //    Many Electron buttons contain an AXImage child whose AXDOMClassList
        //    has the icon identity (e.g. ["icon", "icon-tabler", "icon-tabler-home"]).
        if let children = getAttribute(element, kAXChildrenAttribute) as? [AXUIElement] {
            for child in children {
                let childRole = getAttribute(child, kAXRoleAttribute) as? String
                if childRole == "AXStaticText" {
                    if let val = getAttribute(child, kAXValueAttribute) as? String, !val.isEmpty {
                        return val
                    }
                }
                if childRole == "AXImage" {
                    if let classList = getAttribute(child, "AXDOMClassList") as? [String],
                        !classList.isEmpty,
                        let iconName = Self.iconClassParser.parse(classList)
                    {
                        return iconName
                    }
                }
            }
        }

        // 3. AXHelp -- descriptive help text, sometimes the only label
        if let help = getAttribute(element, "AXHelp") as? String, !help.isEmpty {
            return help
        }

        // 4. AXPlaceholderValue -- text field placeholder (e.g. "Search...")
        if let placeholder = getAttribute(element, "AXPlaceholderValue") as? String,
            !placeholder.isEmpty
        {
            return placeholder
        }

        // 5. AXDOMClassList -- extract icon names from CSS classes (Electron apps).
        //    Lucide, Tabler, FontAwesome, etc. encode icon identity in class names.
        if let classList = getAttribute(element, "AXDOMClassList") as? [String], !classList.isEmpty {
            if let iconName = Self.iconClassParser.parse(classList) {
                return iconName
            }
        }

        // 6. AXRoleDescription -- use when more specific than the generic role.
        //    e.g. "close button" is better than just "button" for AXButton.
        if let roleDesc = getAttribute(element, kAXRoleDescriptionAttribute) as? String,
            !roleDesc.isEmpty, !Self.genericRoleDescriptions.contains(roleDesc)
        {
            return roleDesc
        }

        return nil
    }

    /// Role descriptions that are too generic to use as names.
    private static let genericRoleDescriptions: Set<String> = [
        "button", "link", "text field", "text entry area", "image",
        "menu item", "check box", "radio button", "tab", "cell",
        "slider", "pop up button", "combo box", "menu button",
        "incrementor", "color well", "disclosure triangle",
        "switch", "toggle", "group", "list", "table", "outline",
        "scroll area", "scroll bar", "toolbar", "menu bar",
        "menu bar item", "window", "sheet", "drawer",
        "application", "browser", "row", "column",
        "heading", "static text", "tree item",
    ]

    private static let iconClassParser = IconClassParser()

    /// Return nil for empty strings -- AX APIs often return "" rather than nil.
    private func nonEmpty(_ s: String?) -> String? {
        guard let s, !s.isEmpty else { return nil }
        return s
    }

    internal func getAttribute(_ element: AXUIElement, _ attribute: String) -> Any? {
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
        guard result == .success else { return nil }
        return value
    }

    internal func getPosition(of element: AXUIElement) -> CGPoint? {
        guard let value = getAttribute(element, kAXPositionAttribute) else { return nil }
        var point = CGPoint.zero
        // swiftlint:disable:next force_cast
        AXValueGetValue(value as! AXValue, .cgPoint, &point)
        return point
    }

    internal func getSize(of element: AXUIElement) -> CGSize? {
        guard let value = getAttribute(element, kAXSizeAttribute) else { return nil }
        var size = CGSize.zero
        // swiftlint:disable:next force_cast
        AXValueGetValue(value as! AXValue, .cgSize, &size)
        return size
    }

}
