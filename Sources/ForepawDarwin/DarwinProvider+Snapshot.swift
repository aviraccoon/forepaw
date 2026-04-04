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

        let pruning = TreePruning(
            skipMenuBar: options.skipMenuBar, skipZeroSize: options.skipZeroSize)

        // Single-pass tree walk: builds the ElementNode tree and collects
        // AXUIElement handles for interactive elements simultaneously.
        // Previously this was two separate walks, doubling IPC calls.
        var axElements: [Int: AXUIElement] = [:]
        var axCounter = 1
        let root = buildTree(
            element: appElement, depth: 0, maxDepth: effectiveDepth,
            pruning: pruning, axElements: &axElements, axCounter: &axCounter)

        let assigner = RefAssigner()
        let result = assigner.assign(root: root, interactiveOnly: options.interactiveOnly)

        // Map refs to AXUIElement handles collected during the walk.
        refTable.removeAll()
        for (ref, _) in result.refs {
            if let axElement = axElements[ref.id] {
                refTable[ref] = axElement
            }
        }

        // Look up window bounds for window-relative coordinate display.
        // Errors are non-fatal -- we still return the tree without bounds.
        let windowBounds: Rect?
        if let resolved = try? findWindow(pid: runningApp.processIdentifier, window: nil) {
            windowBounds = Rect(
                x: resolved.origin.x, y: resolved.origin.y,
                width: resolved.size.width, height: resolved.size.height)
        } else {
            windowBounds = nil
        }

        return ElementTree(
            app: appName, root: result.root, refs: result.refs,
            windowBounds: windowBounds)
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

    /// Options controlling which subtrees to skip during tree walks.
    internal struct TreePruning {
        let skipMenuBar: Bool
        let skipZeroSize: Bool

        static let none = TreePruning(skipMenuBar: false, skipZeroSize: false)
    }

    // MARK: - Batched attribute fetching

    /// Attributes fetched in a single IPC call per element.
    /// Using AXUIElementCopyMultipleAttributeValues collapses 7+ round-trips into 1.
    private static let batchAttributes: [String] = [
        kAXRoleAttribute,  // 0
        kAXTitleAttribute,  // 1
        kAXDescriptionAttribute,  // 2
        kAXValueAttribute,  // 3
        kAXPositionAttribute,  // 4
        kAXSizeAttribute,  // 5
        kAXChildrenAttribute,  // 6
        kAXSubroleAttribute,  // 7
    ]

    // nonisolated(unsafe): this CFArray is created once from static strings and never mutated.
    nonisolated(unsafe) private static let batchCFArray: CFArray =
        batchAttributes.map {
            $0 as CFString
        } as CFArray

    /// Fetch multiple attributes in a single IPC call.
    /// Returns an array of values (or kCFNull for missing attributes).
    private func getMultipleAttributes(_ element: AXUIElement) -> [Any?] {
        var values: CFArray?
        let result = AXUIElementCopyMultipleAttributeValues(
            element, Self.batchCFArray,
            // .stopOnError would abort on first missing attr; 0 means continue.
            AXCopyMultipleAttributeOptions(rawValue: 0),
            &values)
        guard result == .success, let array = values as? [Any?] else {
            return Array(repeating: nil, count: Self.batchAttributes.count)
        }
        // Replace kCFNull with nil for ergonomics.
        return array.map { val in
            if val is NSNull { return nil }
            return val
        }
    }

    // MARK: - Single-pass tree build

    internal func buildTree(
        element: AXUIElement, depth: Int, maxDepth: Int,
        pruning: TreePruning = .none,
        axElements: inout [Int: AXUIElement],
        axCounter: inout Int
    ) -> ElementNode {
        guard depth < maxDepth else { return ElementNode(role: "AXGroup") }

        // Batch fetch: one IPC call for role + title + desc + value + pos + size + children + subrole.
        let attrs = getMultipleAttributes(element)
        let role = attrs[0] as? String ?? "AXUnknown"

        // Skip menu bar subtree if requested.
        if pruning.skipMenuBar && role == "AXMenuBar" {
            return ElementNode(role: role)
        }

        // Collect AXUIElement handle if this element is interactive.
        // Must happen in the same depth-first order as RefAssigner.
        if ElementNode.isInteractiveRole(role) {
            axElements[axCounter] = element
            axCounter += 1
        }

        let name =
            nonEmpty(attrs[1] as? String)
            ?? nonEmpty(attrs[2] as? String)
            ?? computedName(of: element, role: role)

        let value = (attrs[3]).flatMap { val -> String? in
            if let s = val as? String { return s }
            if let n = val as? NSNumber { return n.stringValue }
            return nil
        }

        var bounds: Rect?
        if let posValue = attrs[4], let sizeValue = attrs[5] {
            var point = CGPoint.zero
            var size = CGSize.zero
            // swiftlint:disable force_cast
            AXValueGetValue(posValue as! AXValue, .cgPoint, &point)
            AXValueGetValue(sizeValue as! AXValue, .cgSize, &size)
            // swiftlint:enable force_cast
            bounds = Rect(x: point.x, y: point.y, width: size.width, height: size.height)
        }

        // Collect extra attributes that give agents useful context.
        var attributes: [String: String] = [:]
        if let subrole = attrs[7] as? String, !subrole.isEmpty, subrole != "AXNone" {
            attributes["subrole"] = subrole
        }

        // Skip zero-size subtrees if requested. Elements at 0x0 are collapsed menus,
        // hidden panels, or offscreen content -- walking their children is pure waste.
        if pruning.skipZeroSize, let b = bounds,
            b.width == 0, b.height == 0, depth > 1
        {
            return ElementNode(
                role: role, name: name, value: value, bounds: bounds,
                attributes: attributes)
        }

        var children: [ElementNode] = []
        if let childrenRef = attrs[6] as? [AXUIElement] {
            children = childrenRef.map {
                buildTree(
                    element: $0, depth: depth + 1, maxDepth: maxDepth,
                    pruning: pruning, axElements: &axElements, axCounter: &axCounter)
            }
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

    // MARK: - Ref resolution (action commands)

    /// Walk the AX tree to map ref positions to AXUIElement handles.
    /// Used by resolveRef for action dispatch across CLI invocations.
    /// Must mirror the depth-first order used by RefAssigner.
    ///
    /// Note: this still uses individual getAttribute calls (no batching)
    /// because it only needs role + children -- not the full attribute set.
    /// It's also used without pruning options (action commands don't pass
    /// snapshot options).
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
                    element: child, depth: depth + 1, maxDepth: maxDepth, counter: &counter,
                    elements: &elements)
            }
        }
    }

    // MARK: - Name computation

    /// Derive a name from multiple fallback sources.
    ///
    /// The chain (in priority order):
    /// 1. AXTitleUIElement -> its value or title
    /// 2. First AXStaticText child's value, or AXImage child with icon classes
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

    // MARK: - Helpers

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
