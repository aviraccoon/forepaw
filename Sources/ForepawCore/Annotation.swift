/// Data model for screenshot annotations.
///
/// Platform-agnostic: describes what to annotate and how.
/// Rendering is handled by platform-specific code (e.g. CoreGraphics on macOS).

/// A single element annotation: a labeled marker on a screenshot.
public struct Annotation: Sendable {
    /// The element ref (e.g. @e5).
    public let ref: ElementRef
    /// Sequential display number (1, 2, 3...) for visual compactness.
    public let displayNumber: Int
    /// AX role (e.g. "AXButton").
    public let role: String
    /// Human-readable name, if available.
    public let name: String?
    /// Element bounds in screen coordinates.
    public let bounds: Rect

    public init(ref: ElementRef, displayNumber: Int, role: String, name: String?, bounds: Rect) {
        self.ref = ref
        self.displayNumber = displayNumber
        self.role = role
        self.name = name
        self.bounds = bounds
    }

    /// Short role label for display (strips "AX" prefix).
    public var shortRole: String {
        if role.hasPrefix("AX") {
            return String(role.dropFirst(2))
        }
        return role
    }
}

/// Visual style for screenshot annotations.
public enum AnnotationStyle: String, Sendable, CaseIterable {
    /// Small numbered badges at element positions. Compact legend.
    /// Optimized for AI agents -- minimal visual noise.
    case badges

    /// Colored bounding boxes with role + name labels.
    /// Color-coded by element type. Optimized for human readability.
    case labeled

    /// Dims everything except annotated elements.
    /// Useful for focusing attention on specific elements.
    case spotlight
}

/// Category for color-coding elements by type.
public enum AnnotationCategory: Sendable {
    case button
    case textInput
    case selection  // checkbox, radio, switch, combo, popup
    case navigation  // link, tab, menu item
    case other

    public init(role: String) {
        switch role {
        case "AXButton", "AXMenuButton", "AXDockItem", "AXIncrementor":
            self = .button
        case "AXTextField", "AXTextArea":
            self = .textInput
        case "AXCheckBox", "AXRadioButton", "AXSwitch", "AXComboBox",
            "AXPopUpButton", "AXSlider", "AXColorWell":
            self = .selection
        case "AXLink", "AXTab", "AXMenuItem", "AXTreeItem":
            self = .navigation
        default:
            self = .other
        }
    }
}

/// Collects annotations from an element tree.
///
/// Walks the tree depth-first (matching `RefAssigner` order) and collects
/// annotations for interactive elements that have bounds.
public struct AnnotationCollector: Sendable {
    public init() {}

    /// Collect annotations for all interactive elements with bounds.
    ///
    /// - Parameters:
    ///   - tree: The element tree (with refs already assigned).
    ///   - windowBounds: The window's screen-space bounds, used to filter
    ///     off-screen elements and convert to window-relative coordinates.
    /// - Returns: Annotations sorted by display number.
    public func collect(from tree: ElementTree, windowBounds: Rect) -> [Annotation] {
        var annotations: [Annotation] = []
        var displayNumber = 1
        walk(
            node: tree.root, annotations: &annotations, displayNumber: &displayNumber,
            windowBounds: windowBounds)
        return annotations
    }

    private func walk(
        node: ElementNode,
        annotations: inout [Annotation],
        displayNumber: inout Int,
        windowBounds: Rect
    ) {
        if let ref = node.ref, let bounds = node.bounds, node.isInteractive {
            // Convert to window-relative coordinates
            let relativeBounds = Rect(
                x: bounds.x - windowBounds.x,
                y: bounds.y - windowBounds.y,
                width: bounds.width,
                height: bounds.height
            )

            // Only include elements that overlap the window area
            let windowWidth = windowBounds.width
            let windowHeight = windowBounds.height
            if relativeBounds.x + relativeBounds.width > 0
                && relativeBounds.y + relativeBounds.height > 0
                && relativeBounds.x < windowWidth
                && relativeBounds.y < windowHeight
            {
                annotations.append(
                    Annotation(
                        ref: ref,
                        displayNumber: displayNumber,
                        role: node.role,
                        name: node.name,
                        bounds: relativeBounds
                    )
                )
                displayNumber += 1
            }
        }

        for child in node.children {
            walk(
                node: child, annotations: &annotations, displayNumber: &displayNumber,
                windowBounds: windowBounds)
        }
    }
}

/// Formats the text legend for annotations.
public struct AnnotationLegend: Sendable {
    public init() {}

    /// Generate a compact legend mapping display numbers to refs and element info.
    public func format(annotations: [Annotation]) -> String {
        annotations.map { a in
            let name: String
            if let n = a.name, !n.isEmpty {
                name = " \"\(n)\""
            } else {
                name = ""
            }
            return "[\(a.displayNumber)] \(a.ref) \(a.shortRole)\(name)"
        }.joined(separator: "\n")
    }
}
