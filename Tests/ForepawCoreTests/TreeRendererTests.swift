import Testing

@testable import ForepawCore

@Suite("TreeRenderer")
struct TreeRendererTests {
    @Test("renders simple tree with roles and names")
    func simpleTree() {
        let tree = ElementTree(
            app: "TestApp",
            root: ElementNode(
                role: "AXWindow",
                name: "Main Window",
                children: [
                    ElementNode(role: "AXButton", name: "OK", ref: ElementRef(1)),
                    ElementNode(role: "AXTextField", name: "Name", value: "hello", ref: ElementRef(2)),
                ]
            ),
            refs: [:]
        )

        let renderer = TreeRenderer()
        let output = renderer.render(tree: tree)
        let lines = output.split(separator: "\n").map(String.init)

        #expect(lines[0] == "app: TestApp")
        #expect(lines[1] == "window \"Main Window\"")
        #expect(lines[2] == "  button @e1 \"OK\"")
        #expect(lines[3] == "  textfield @e2 \"Name\" value=\"hello\"")
    }

    @Test("strips AX prefix from roles")
    func stripsAXPrefix() {
        let tree = ElementTree(
            app: "App",
            root: ElementNode(role: "AXSplitGroup"),
            refs: [:]
        )

        let renderer = TreeRenderer()
        let output = renderer.render(tree: tree)

        #expect(output.contains("splitgroup"))
        #expect(!output.contains("AXSplitGroup"))
    }

    @Test("truncates long values")
    func truncatesLongValues() {
        let longValue = String(repeating: "x", count: 100)
        let tree = ElementTree(
            app: "App",
            root: ElementNode(role: "AXTextField", value: longValue),
            refs: [:]
        )

        let renderer = TreeRenderer()
        let output = renderer.render(tree: tree)

        #expect(output.contains("..."))
        #expect(!output.contains(longValue))
    }

    @Test("renders nested tree with proper indentation")
    func nestedIndentation() {
        let tree = ElementTree(
            app: "App",
            root: ElementNode(
                role: "AXWindow",
                children: [
                    ElementNode(
                        role: "AXGroup",
                        children: [
                            ElementNode(role: "AXButton", name: "Deep")
                        ]
                    )
                ]
            ),
            refs: [:]
        )

        let renderer = TreeRenderer()
        let output = renderer.render(tree: tree)
        let lines = output.split(separator: "\n").map(String.init)

        #expect(lines[1] == "window")
        #expect(lines[2] == "  group")
        #expect(lines[3] == "    button \"Deep\"")
    }

    @Test("omits empty name and value")
    func omitsEmptyFields() {
        let tree = ElementTree(
            app: "App",
            root: ElementNode(role: "AXGroup"),
            refs: [:]
        )

        let renderer = TreeRenderer()
        let output = renderer.render(tree: tree)

        #expect(output == "app: App\ngroup")
    }
}
