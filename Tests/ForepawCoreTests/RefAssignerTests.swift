import Testing

@testable import ForepawCore

@Suite("RefAssigner")
struct RefAssignerTests {
    @Test("assigns refs to interactive elements in depth-first order")
    func assignsRefsDepthFirst() {
        let tree = ElementNode(
            role: "AXWindow",
            name: "Test Window",
            children: [
                ElementNode(
                    role: "AXGroup",
                    children: [
                        ElementNode(role: "AXButton", name: "OK"),
                        ElementNode(role: "AXButton", name: "Cancel"),
                    ]),
                ElementNode(role: "AXTextField", name: "Name"),
            ]
        )

        let assigner = RefAssigner()
        let result = assigner.assign(root: tree, interactiveOnly: false)

        // Window (non-interactive) -> Group (non-interactive) -> Button OK (@e1) -> Button Cancel (@e2) -> TextField (@e3)
        #expect(result.root.children[0].children[0].ref == ElementRef(1))
        #expect(result.root.children[0].children[1].ref == ElementRef(2))
        #expect(result.root.children[1].ref == ElementRef(3))
        #expect(result.root.ref == nil)  // Window is not interactive
    }

    @Test("interactive-only mode prunes non-interactive leaves")
    func interactiveOnlyPrunes() {
        let tree = ElementNode(
            role: "AXWindow",
            children: [
                ElementNode(
                    role: "AXGroup",
                    children: [
                        ElementNode(role: "AXStaticText", name: "Label"),
                        ElementNode(role: "AXButton", name: "OK"),
                    ]),
                ElementNode(role: "AXStaticText", name: "Footer"),
            ]
        )

        let assigner = RefAssigner()
        let result = assigner.assign(root: tree, interactiveOnly: true)

        // Footer (static text, no interactive children) should be pruned
        // Group stays because it has the button
        #expect(result.root.children.count == 1)  // Only the group with the button
        #expect(result.root.children[0].children.count == 1)  // Only the button, label pruned
    }

    @Test("ElementRef parsing")
    func refParsing() {
        #expect(ElementRef.parse("@e3") == ElementRef(3))
        #expect(ElementRef.parse("@e42") == ElementRef(42))
        #expect(ElementRef.parse("e3") == nil)
        #expect(ElementRef.parse("@x3") == nil)
        #expect(ElementRef.parse("") == nil)
    }

}
