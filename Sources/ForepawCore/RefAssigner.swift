/// Assigns `@e1`, `@e2`, etc. to interactive elements in depth-first order.
///
/// Platform-agnostic: works on any `ElementNode` tree regardless of
/// how it was constructed.
public struct RefAssigner: Sendable {
    public init() {}

    public struct Result: Sendable {
        public let root: ElementNode
        public let refs: [ElementRef: ElementRefInfo]
    }

    /// Walk the tree, assigning refs to interactive elements.
    /// Returns a new tree with refs populated and a ref lookup table.
    public func assign(root: ElementNode, interactiveOnly: Bool) -> Result {
        var counter = 1
        var refs: [ElementRef: ElementRefInfo] = [:]
        let newRoot = walk(node: root, counter: &counter, refs: &refs, interactiveOnly: interactiveOnly)
        return Result(root: newRoot, refs: refs)
    }

    private func walk(
        node: ElementNode,
        counter: inout Int,
        refs: inout [ElementRef: ElementRefInfo],
        interactiveOnly: Bool
    ) -> ElementNode {
        var ref: ElementRef?
        if node.isInteractive {
            let elementRef = ElementRef(counter)
            ref = elementRef
            // Platform handle is already set by the provider during tree construction
            if let handle = node.ref.map({ _ in PlatformHandle(counter) }) {
                refs[elementRef] = ElementRefInfo(role: node.role, name: node.name, handle: handle)
            }
            counter += 1
        }

        let children: [ElementNode]
        if interactiveOnly {
            // Keep non-interactive parents if they have interactive descendants
            children = node.children.compactMap { child in
                let walked = walk(node: child, counter: &counter, refs: &refs, interactiveOnly: true)
                if walked.ref != nil || !walked.children.isEmpty {
                    return walked
                }
                return nil
            }
        } else {
            children = node.children.map { child in
                walk(node: child, counter: &counter, refs: &refs, interactiveOnly: false)
            }
        }

        return ElementNode(
            role: node.role,
            name: node.name,
            value: node.value,
            ref: ref ?? node.ref,
            bounds: node.bounds,
            attributes: node.attributes,
            children: children
        )
    }
}
