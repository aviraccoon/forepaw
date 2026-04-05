/// A node in the accessibility element tree.
public struct ElementNode: Sendable {
    public let role: String
    public let name: String?
    public let value: String?
    public let ref: ElementRef?
    public let bounds: Rect?
    public let attributes: [String: String]
    public let children: [ElementNode]

    public init(
        role: String,
        name: String? = nil,
        value: String? = nil,
        ref: ElementRef? = nil,
        bounds: Rect? = nil,
        attributes: [String: String] = [:],
        children: [ElementNode] = []
    ) {
        self.role = role
        self.name = name
        self.value = value
        self.ref = ref
        self.bounds = bounds
        self.attributes = attributes
        self.children = children
    }

    /// Whether this element is interactive (should receive a ref).
    public var isInteractive: Bool {
        Self.interactiveRoles.contains(role)
    }

    /// Static check for use by providers during tree construction.
    public static func isInteractiveRole(_ role: String) -> Bool {
        interactiveRoles.contains(role)
    }

    public static let interactiveRoles: Set<String> = [
        "AXButton", "AXTextField", "AXTextArea", "AXCheckBox",
        "AXRadioButton", "AXSlider", "AXComboBox", "AXPopUpButton",
        "AXMenuButton", "AXLink", "AXMenuItem", "AXTab",
        "AXSwitch", "AXIncrementor", "AXColorWell", "AXTreeItem",
        "AXCell", "AXDockItem",
    ]
}

/// The full accessibility tree for a window/app.
public struct ElementTree: Sendable {
    public let app: String
    public let root: ElementNode
    /// All refs assigned in this snapshot, in order.
    public let refs: [ElementRef: ElementRefInfo]
    /// Window bounds in screen coordinates. Used to convert element bounds
    /// to window-relative coordinates for display and input.
    public let windowBounds: Rect?
    /// Performance timing breakdown, populated when `SnapshotOptions.timing` is true.
    public let timing: SnapshotTiming?

    public init(
        app: String, root: ElementNode, refs: [ElementRef: ElementRefInfo],
        windowBounds: Rect? = nil, timing: SnapshotTiming? = nil
    ) {
        self.app = app
        self.root = root
        self.refs = refs
        self.windowBounds = windowBounds
        self.timing = timing
    }
}

/// Performance timing for a snapshot.
public struct SnapshotTiming: Sendable {
    /// Total wall time for the tree walk in milliseconds.
    public let totalMs: Double
    /// Total number of nodes visited.
    public let nodeCount: Int
    /// The root of the tree (for adaptive breakdown).
    public let root: ElementNode

    public init(totalMs: Double, nodeCount: Int, root: ElementNode) {
        self.totalMs = totalMs
        self.nodeCount = nodeCount
        self.root = root
    }

    /// Format timing as a human-readable report.
    /// Adaptively expands subtrees that hold >10% of total nodes.
    public func report() -> String {
        var lines: [String] = []
        let avg = nodeCount > 0 ? totalMs / Double(nodeCount) : 0
        lines.append(
            String(
                format: "snapshot: %.0fms, %d nodes, %.1fms/node avg",
                totalMs, nodeCount, avg))
        let threshold = max(nodeCount / 10, 2)  // 10% or at least 2
        appendSubtreeReport(root, indent: 0, total: nodeCount, threshold: threshold, into: &lines)
        return lines.joined(separator: "\n")
    }

    private func appendSubtreeReport(
        _ node: ElementNode, indent: Int, total: Int,
        threshold: Int, into lines: inout [String]
    ) {
        for child in node.children {
            let count = Self.countNodes(child)
            if count < threshold { continue }  // skip small subtrees

            // Skip single-child chains: if this node has exactly one large child,
            // don't print this node -- just recurse into the child. This collapses
            // Electron's deep wrapper nesting (group > group > group > ...) into
            // the first node that actually branches.
            let largeChildren = child.children.filter { Self.countNodes($0) >= threshold }
            if largeChildren.count == 1 && child.name == nil {
                appendSubtreeReport(
                    child, indent: indent, total: total,
                    threshold: threshold, into: &lines)
                continue
            }

            let pct = total > 0 ? Double(count) / Double(total) * 100 : 0
            let label = Self.nodeLabel(child)
            let prefix = String(repeating: "  ", count: indent + 1)
            lines.append(
                "\(prefix)\(label) \(String(format: "%5d nodes  %5.1f%%", count, pct))")
            // Keep expanding if this subtree is still large
            if count >= threshold && !child.children.isEmpty {
                appendSubtreeReport(
                    child, indent: indent + 1, total: total,
                    threshold: threshold, into: &lines)
            }
        }
    }

    private static func nodeLabel(_ node: ElementNode) -> String {
        let name = node.name.flatMap { $0.isEmpty ? nil : $0 }
        let label = name.map { "\(node.role) \"\($0)\"" } ?? node.role
        return String(label.prefix(40))
    }

    /// Count total nodes in a subtree.
    public static func countNodes(_ node: ElementNode) -> Int {
        1 + node.children.reduce(0) { $0 + countNodes($1) }
    }
}

/// Opaque reference to an interactive element, valid until the next snapshot.
public struct ElementRef: Sendable, Hashable, Codable, CustomStringConvertible {
    public let id: Int

    public init(_ id: Int) {
        self.id = id
    }

    public var description: String { "@e\(id)" }

    /// Parse a ref string like "@e3" into an ElementRef.
    public static func parse(_ string: String) -> ElementRef? {
        let trimmed = string.trimmingCharacters(in: .whitespaces)
        guard trimmed.hasPrefix("@e"),
            let id = Int(trimmed.dropFirst(2))
        else { return nil }
        return ElementRef(id)
    }
}

/// Info stored alongside a ref for action dispatch.
public struct ElementRefInfo: Sendable {
    public let role: String
    public let name: String?
    /// Opaque handle the platform provider uses to find this element again.
    public let handle: PlatformHandle

    public init(role: String, name: String?, handle: PlatformHandle) {
        self.role = role
        self.name = name
        self.handle = handle
    }
}

/// Type-erased platform handle. Each provider defines what this contains.
public struct PlatformHandle: Sendable {
    public let value: any Sendable

    public init(_ value: any Sendable) {
        self.value = value
    }
}
