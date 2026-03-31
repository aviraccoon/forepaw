/// Renders an `ElementNode` tree as indented text, matching the
/// agent-browser snapshot format.
public struct TreeRenderer: Sendable {
    public init() {}

    public func render(tree: ElementTree) -> String {
        var lines: [String] = []
        lines.append("app: \(tree.app)")
        renderNode(tree.root, indent: 0, lines: &lines)
        return lines.joined(separator: "\n")
    }

    private func renderNode(_ node: ElementNode, indent: Int, lines: inout [String]) {
        let prefix = String(repeating: "  ", count: indent)
        var parts: [String] = []

        // Role (strip AX prefix for readability)
        let role = node.role.hasPrefix("AX") ? String(node.role.dropFirst(2)).lowercased() : node.role
        parts.append(role)

        // Ref
        if let ref = node.ref {
            parts.append(ref.description)
        }

        // Name
        if let name = node.name, !name.isEmpty {
            parts.append("\"\(name)\"")
        }

        // Value (truncated for display)
        if let value = node.value, !value.isEmpty {
            let display = value.count > 80 ? String(value.prefix(77)) + "..." : value
            parts.append("value=\"\(display)\"")
        }

        // Extra attributes
        for (key, val) in node.attributes.sorted(by: { $0.key < $1.key }) {
            parts.append("\(key)=\(val)")
        }

        lines.append(prefix + parts.joined(separator: " "))

        for child in node.children {
            renderNode(child, indent: indent + 1, lines: &lines)
        }
    }
}
