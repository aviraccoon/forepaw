import Testing

@testable import ForepawCore

@Suite("AnnotationCategory")
struct AnnotationCategoryTests {
    @Test("button roles")
    func buttonRoles() {
        #expect(AnnotationCategory(role: "AXButton") == .button)
        #expect(AnnotationCategory(role: "AXMenuButton") == .button)
        #expect(AnnotationCategory(role: "AXDockItem") == .button)
        #expect(AnnotationCategory(role: "AXIncrementor") == .button)
    }

    @Test("text input roles")
    func textInputRoles() {
        #expect(AnnotationCategory(role: "AXTextField") == .textInput)
        #expect(AnnotationCategory(role: "AXTextArea") == .textInput)
    }

    @Test("selection roles")
    func selectionRoles() {
        #expect(AnnotationCategory(role: "AXCheckBox") == .selection)
        #expect(AnnotationCategory(role: "AXRadioButton") == .selection)
        #expect(AnnotationCategory(role: "AXSlider") == .selection)
        #expect(AnnotationCategory(role: "AXComboBox") == .selection)
        #expect(AnnotationCategory(role: "AXPopUpButton") == .selection)
        #expect(AnnotationCategory(role: "AXSwitch") == .selection)
        #expect(AnnotationCategory(role: "AXColorWell") == .selection)
    }

    @Test("navigation roles")
    func navigationRoles() {
        #expect(AnnotationCategory(role: "AXLink") == .navigation)
        #expect(AnnotationCategory(role: "AXTab") == .navigation)
        #expect(AnnotationCategory(role: "AXMenuItem") == .navigation)
        #expect(AnnotationCategory(role: "AXTreeItem") == .navigation)
    }

    @Test("unknown roles fall back to other")
    func unknownRoles() {
        #expect(AnnotationCategory(role: "AXGroup") == .other)
        #expect(AnnotationCategory(role: "AXImage") == .other)
        #expect(AnnotationCategory(role: "AXUnknown") == .other)
    }
}

@Suite("Annotation")
struct AnnotationTests {
    @Test("shortRole strips AX prefix")
    func shortRoleStripsPrefix() {
        let a = Annotation(
            ref: ElementRef(5), displayNumber: 1,
            role: "AXButton", name: "Save",
            bounds: Rect(x: 0, y: 0, width: 100, height: 30)
        )
        #expect(a.shortRole == "Button")
    }

    @Test("shortRole preserves non-AX role")
    func shortRolePreservesNonAX() {
        let a = Annotation(
            ref: ElementRef(1), displayNumber: 1,
            role: "CustomRole", name: nil,
            bounds: Rect(x: 0, y: 0, width: 50, height: 50)
        )
        #expect(a.shortRole == "CustomRole")
    }
}

@Suite("AnnotationStyle")
struct AnnotationStyleTests {
    @Test("raw values for CLI parsing")
    func rawValues() {
        #expect(AnnotationStyle(rawValue: "badges") == .badges)
        #expect(AnnotationStyle(rawValue: "labeled") == .labeled)
        #expect(AnnotationStyle(rawValue: "spotlight") == .spotlight)
        #expect(AnnotationStyle(rawValue: "invalid") == nil)
    }

    @Test("allCases includes all styles")
    func allCases() {
        #expect(AnnotationStyle.allCases.count == 3)
    }
}

@Suite("AnnotationCollector")
struct AnnotationCollectorTests {
    let windowBounds = Rect(x: 100, y: 50, width: 800, height: 600)

    func makeNode(
        role: String, name: String? = nil, bounds: Rect? = nil,
        ref: ElementRef? = nil, children: [ElementNode] = []
    ) -> ElementNode {
        ElementNode(
            role: role, name: name, ref: ref, bounds: bounds,
            children: children
        )
    }

    @Test("collects interactive elements with bounds")
    func collectsInteractive() {
        let tree = ElementTree(
            app: "TestApp",
            root: makeNode(
                role: "AXWindow",
                children: [
                    makeNode(
                        role: "AXButton", name: "Save",
                        bounds: Rect(x: 200, y: 100, width: 80, height: 30),
                        ref: ElementRef(1)
                    ),
                    makeNode(
                        role: "AXTextField", name: "Search",
                        bounds: Rect(x: 300, y: 100, width: 200, height: 25),
                        ref: ElementRef(2)
                    ),
                ]),
            refs: [:]
        )

        let collector = AnnotationCollector()
        let annotations = collector.collect(from: tree, windowBounds: windowBounds)

        #expect(annotations.count == 2)
        #expect(annotations[0].displayNumber == 1)
        #expect(annotations[0].ref == ElementRef(1))
        #expect(annotations[0].name == "Save")
        #expect(annotations[1].displayNumber == 2)
        #expect(annotations[1].ref == ElementRef(2))
    }

    @Test("converts to window-relative coordinates")
    func windowRelativeCoords() {
        let tree = ElementTree(
            app: "TestApp",
            root: makeNode(
                role: "AXWindow",
                children: [
                    makeNode(
                        role: "AXButton", name: "OK",
                        bounds: Rect(x: 250, y: 150, width: 60, height: 30),
                        ref: ElementRef(1)
                    )
                ]),
            refs: [:]
        )

        let collector = AnnotationCollector()
        let annotations = collector.collect(from: tree, windowBounds: windowBounds)

        #expect(annotations.count == 1)
        // 250 - 100 = 150, 150 - 50 = 100
        #expect(annotations[0].bounds.x == 150)
        #expect(annotations[0].bounds.y == 100)
    }

    @Test("skips elements without bounds")
    func skipsNoBounds() {
        let tree = ElementTree(
            app: "TestApp",
            root: makeNode(
                role: "AXWindow",
                children: [
                    makeNode(role: "AXButton", name: "Ghost", ref: ElementRef(1))
                ]),
            refs: [:]
        )

        let collector = AnnotationCollector()
        let annotations = collector.collect(from: tree, windowBounds: windowBounds)
        #expect(annotations.isEmpty)
    }

    @Test("skips non-interactive elements")
    func skipsNonInteractive() {
        let tree = ElementTree(
            app: "TestApp",
            root: makeNode(
                role: "AXWindow",
                children: [
                    makeNode(
                        role: "AXStaticText", name: "Label",
                        bounds: Rect(x: 200, y: 100, width: 80, height: 20)
                    )
                ]),
            refs: [:]
        )

        let collector = AnnotationCollector()
        let annotations = collector.collect(from: tree, windowBounds: windowBounds)
        #expect(annotations.isEmpty)
    }

    @Test("skips off-screen elements")
    func skipsOffScreen() {
        let tree = ElementTree(
            app: "TestApp",
            root: makeNode(
                role: "AXWindow",
                children: [
                    // Entirely to the left of the window
                    makeNode(
                        role: "AXButton", name: "Hidden",
                        bounds: Rect(x: 0, y: 100, width: 50, height: 30),
                        ref: ElementRef(1)
                    ),
                    // Entirely below the window
                    makeNode(
                        role: "AXButton", name: "Below",
                        bounds: Rect(x: 200, y: 700, width: 80, height: 30),
                        ref: ElementRef(2)
                    ),
                    // Visible
                    makeNode(
                        role: "AXButton", name: "Visible",
                        bounds: Rect(x: 200, y: 100, width: 80, height: 30),
                        ref: ElementRef(3)
                    ),
                ]),
            refs: [:]
        )

        let collector = AnnotationCollector()
        let annotations = collector.collect(from: tree, windowBounds: windowBounds)

        #expect(annotations.count == 1)
        #expect(annotations[0].name == "Visible")
    }

    @Test("sequential display numbers skip gaps in refs")
    func sequentialDisplayNumbers() {
        let tree = ElementTree(
            app: "TestApp",
            root: makeNode(
                role: "AXWindow",
                children: [
                    makeNode(
                        role: "AXButton", name: "A",
                        bounds: Rect(x: 200, y: 100, width: 80, height: 30),
                        ref: ElementRef(5)
                    ),
                    makeNode(
                        role: "AXButton", name: "B",
                        bounds: Rect(x: 300, y: 100, width: 80, height: 30),
                        ref: ElementRef(10)
                    ),
                ]),
            refs: [:]
        )

        let collector = AnnotationCollector()
        let annotations = collector.collect(from: tree, windowBounds: windowBounds)

        #expect(annotations[0].displayNumber == 1)
        #expect(annotations[0].ref == ElementRef(5))
        #expect(annotations[1].displayNumber == 2)
        #expect(annotations[1].ref == ElementRef(10))
    }
}

@Suite("AnnotationLegend")
struct AnnotationLegendTests {
    @Test("formats legend with names")
    func formatsWithNames() {
        let annotations = [
            Annotation(
                ref: ElementRef(5), displayNumber: 1,
                role: "AXButton", name: "Save",
                bounds: Rect(x: 0, y: 0, width: 80, height: 30)
            ),
            Annotation(
                ref: ElementRef(8), displayNumber: 2,
                role: "AXTextField", name: "Search",
                bounds: Rect(x: 0, y: 0, width: 200, height: 25)
            ),
        ]

        let legend = AnnotationLegend().format(annotations: annotations)
        #expect(legend == "[1] @e5 Button \"Save\"\n[2] @e8 TextField \"Search\"")
    }

    @Test("omits empty names")
    func omitsEmptyNames() {
        let annotations = [
            Annotation(
                ref: ElementRef(1), displayNumber: 1,
                role: "AXButton", name: nil,
                bounds: Rect(x: 0, y: 0, width: 80, height: 30)
            ),
            Annotation(
                ref: ElementRef(2), displayNumber: 2,
                role: "AXButton", name: "",
                bounds: Rect(x: 0, y: 0, width: 80, height: 30)
            ),
        ]

        let legend = AnnotationLegend().format(annotations: annotations)
        #expect(legend == "[1] @e1 Button\n[2] @e2 Button")
    }
}
