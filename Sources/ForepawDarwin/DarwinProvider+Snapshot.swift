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

    internal func buildTree(element: AXUIElement, depth: Int, maxDepth: Int) -> ElementNode {
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

    /// Derive a name from AXTitleUIElement or first AXStaticText child.
    /// Many elements (cells, rows) don't have a direct title but point
    /// to a label element via AXTitleUIElement, or contain a static text child.
    internal func computedName(of element: AXUIElement) -> String? {
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
