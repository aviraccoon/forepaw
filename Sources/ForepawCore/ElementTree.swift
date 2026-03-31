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

    public init(app: String, root: ElementNode, refs: [ElementRef: ElementRefInfo]) {
        self.app = app
        self.root = root
        self.refs = refs
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
