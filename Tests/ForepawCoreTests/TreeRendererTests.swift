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

    @Test("renders bounds as window-relative when window bounds available")
    func rendersBoundsRelative() {
        let tree = ElementTree(
            app: "App",
            root: ElementNode(
                role: "AXWindow",
                name: "Main",
                bounds: Rect(x: 100, y: 200, width: 800, height: 600),
                children: [
                    ElementNode(
                        role: "AXButton",
                        name: "OK",
                        ref: ElementRef(1),
                        bounds: Rect(x: 150, y: 250, width: 80, height: 30)
                    )
                ]
            ),
            refs: [:],
            windowBounds: Rect(x: 100, y: 200, width: 800, height: 600)
        )

        let renderer = TreeRenderer()
        let output = renderer.render(tree: tree)
        let lines = output.split(separator: "\n").map(String.init)

        // Window itself should be at 0,0 relative to itself
        #expect(lines[1] == "window \"Main\" (0,0 800x600)")
        // Button at 150,250 screen -> 50,50 window-relative
        #expect(lines[2] == "  button @e1 \"OK\" (50,50 80x30)")
    }

    @Test("renders bounds as absolute when no window bounds")
    func rendersBoundsAbsolute() {
        let tree = ElementTree(
            app: "App",
            root: ElementNode(
                role: "AXWindow",
                name: "Main",
                bounds: Rect(x: 100, y: 200, width: 800, height: 600),
                children: [
                    ElementNode(
                        role: "AXButton",
                        name: "OK",
                        ref: ElementRef(1),
                        bounds: Rect(x: 150, y: 250, width: 80, height: 30)
                    )
                ]
            ),
            refs: [:]
        )

        let renderer = TreeRenderer()
        let output = renderer.render(tree: tree)
        let lines = output.split(separator: "\n").map(String.init)

        // No windowBounds -> absolute coordinates
        #expect(lines[1] == "window \"Main\" (100,200 800x600)")
        #expect(lines[2] == "  button @e1 \"OK\" (150,250 80x30)")
    }

    @Test("omits bounds when nil")
    func omitsMissingBounds() {
        let tree = ElementTree(
            app: "App",
            root: ElementNode(role: "AXButton", name: "OK", ref: ElementRef(1)),
            refs: [:]
        )

        let renderer = TreeRenderer()
        let output = renderer.render(tree: tree)

        #expect(!output.contains("("))
        #expect(output.contains("button @e1 \"OK\""))
    }
}
