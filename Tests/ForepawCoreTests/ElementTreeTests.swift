import Testing

@testable import ForepawCore

@Suite("ElementTree")
struct ElementTreeTests {
    @Test("interactive roles are correctly identified")
    func interactiveRoles() {
        #expect(ElementNode.isInteractiveRole("AXButton") == true)
        #expect(ElementNode.isInteractiveRole("AXTextField") == true)
        #expect(ElementNode.isInteractiveRole("AXCheckBox") == true)
        #expect(ElementNode.isInteractiveRole("AXLink") == true)
        #expect(ElementNode.isInteractiveRole("AXMenuItem") == true)
        #expect(ElementNode.isInteractiveRole("AXSlider") == true)
        #expect(ElementNode.isInteractiveRole("AXPopUpButton") == true)
        #expect(ElementNode.isInteractiveRole("AXSwitch") == true)
    }

    @Test("non-interactive roles are correctly identified")
    func nonInteractiveRoles() {
        #expect(ElementNode.isInteractiveRole("AXGroup") == false)
        #expect(ElementNode.isInteractiveRole("AXWindow") == false)
        #expect(ElementNode.isInteractiveRole("AXStaticText") == false)
        #expect(ElementNode.isInteractiveRole("AXImage") == false)
        #expect(ElementNode.isInteractiveRole("AXScrollArea") == false)
        #expect(ElementNode.isInteractiveRole("AXUnknown") == false)
        #expect(ElementNode.isInteractiveRole("") == false)
    }

    @Test("isInteractive uses role")
    func isInteractive() {
        let button = ElementNode(role: "AXButton", name: "OK")
        let group = ElementNode(role: "AXGroup")

        #expect(button.isInteractive == true)
        #expect(group.isInteractive == false)
    }

    @Test("ElementRef description format")
    func refDescription() {
        #expect(ElementRef(1).description == "@e1")
        #expect(ElementRef(42).description == "@e42")
        #expect(ElementRef(100).description == "@e100")
    }

    @Test("ElementRef parse roundtrip")
    func refRoundtrip() {
        for i in [1, 5, 42, 100, 999] {
            let ref = ElementRef(i)
            let parsed = ElementRef.parse(ref.description)
            #expect(parsed == ref)
        }
    }

    @Test("ElementRef parse edge cases")
    func refParseEdgeCases() {
        #expect(ElementRef.parse("@e0") == ElementRef(0))
        #expect(ElementRef.parse("  @e5  ") == ElementRef(5))
        #expect(ElementRef.parse("@e") == nil)
        #expect(ElementRef.parse("@") == nil)
        #expect(ElementRef.parse("") == nil)
        // @e-1 parses as ElementRef(-1) -- Int("-1") succeeds. Harmless; negative refs won't match anything.
        #expect(ElementRef.parse("@eabc") == nil)
    }
}
