import Testing

@testable import ForepawCore

@Suite("SnapshotDiffer")
struct SnapshotDiffTests {

    let differ = SnapshotDiffer()

    // MARK: - Ref stripping

    @Test("strips single ref from line")
    func stripsSingleRef() {
        let input = #"  button @e5 "OK" (100,200 80x30)"#
        let expected = #"  button "OK" (100,200 80x30)"#
        #expect(SnapshotDiffer.stripRefs(input) == expected)
    }

    @Test("strips ref at end of line")
    func stripsRefAtEnd() {
        let input = "  button @e12"
        let expected = "  button"
        #expect(SnapshotDiffer.stripRefs(input) == expected)
    }

    @Test("leaves lines without refs unchanged")
    func leavesNonRefLines() {
        let input = "  group \"Settings\""
        #expect(SnapshotDiffer.stripRefs(input) == input)
    }

    @Test("strips ref with large number")
    func stripsLargeRef() {
        let input = "  menuitem @e302 \"Paste\""
        let expected = "  menuitem \"Paste\""
        #expect(SnapshotDiffer.stripRefs(input) == expected)
    }

    // MARK: - Identical snapshots

    @Test("identical snapshots produce no changes")
    func identicalSnapshots() {
        let text = """
            app: Finder
              window "Home"
                button @e1 "Close"
                button @e2 "Minimize"
            """
        let diff = differ.diff(old: text, new: text)
        #expect(diff.isEmpty)
        #expect(diff.added.isEmpty)
        #expect(diff.removed.isEmpty)
        #expect(diff.summary == "no changes")
    }

    // MARK: - Added elements

    @Test("detects added elements")
    func detectsAdded() {
        let old = """
            app: TestApp
              window "Main"
                button @e1 "OK"
            """
        let new = """
            app: TestApp
              window "Main"
                button @e1 "OK"
                button @e2 "Cancel"
            """
        let diff = differ.diff(old: old, new: new)
        #expect(diff.added.count == 1)
        #expect(diff.removed.isEmpty)
        #expect(diff.added[0].text.contains("Cancel"))
    }

    // MARK: - Removed elements

    @Test("detects removed elements")
    func detectsRemoved() {
        let old = """
            app: TestApp
              window "Main"
                button @e1 "OK"
                button @e2 "Cancel"
            """
        let new = """
            app: TestApp
              window "Main"
                button @e1 "OK"
            """
        let diff = differ.diff(old: old, new: new)
        #expect(diff.removed.count == 1)
        #expect(diff.added.isEmpty)
        #expect(diff.removed[0].text.contains("Cancel"))
    }

    // MARK: - Ref shifts don't cause false changes

    @Test("ref shift from added element does not affect subsequent lines")
    func refShiftHandled() {
        let old = """
            app: TestApp
              window "Main"
                button @e1 "Save"
                textfield @e2 "Name"
                button @e3 "Cancel"
            """
        // New element added before Save, shifting all refs by 1
        let new = """
            app: TestApp
              window "Main"
                button @e1 "New"
                button @e2 "Save"
                textfield @e3 "Name"
                button @e4 "Cancel"
            """
        let diff = differ.diff(old: old, new: new)
        // Only the "New" button should be added; Save, Name, Cancel are unchanged
        #expect(diff.added.count == 1)
        #expect(diff.removed.isEmpty)
        #expect(diff.added[0].text.contains("New"))
        // The unchanged lines should have the NEW refs
        let unchangedTexts = diff.unchanged.map(\.text)
        #expect(unchangedTexts.contains { $0.contains("@e2") && $0.contains("Save") })
        #expect(unchangedTexts.contains { $0.contains("@e3") && $0.contains("Name") })
        #expect(unchangedTexts.contains { $0.contains("@e4") && $0.contains("Cancel") })
    }

    // MARK: - Value changes

    @Test("detects value changes on same element")
    func detectsValueChange() {
        let old = """
            app: TestApp
              window "Main"
                textfield @e1 "Search" value="hello"
            """
        let new = """
            app: TestApp
              window "Main"
                textfield @e1 "Search" value="hello world"
            """
        let diff = differ.diff(old: old, new: new)
        // Value change shows as remove old + add new
        #expect(diff.removed.count == 1)
        #expect(diff.added.count == 1)
        #expect(diff.removed[0].text.contains("hello"))
        #expect(diff.added[0].text.contains("hello world"))
    }

    // MARK: - Mixed changes

    @Test("handles simultaneous additions and removals")
    func mixedChanges() {
        let old = """
            app: TestApp
              window "Main"
                button @e1 "Submit"
                button @e2 "Reset"
            """
        let new = """
            app: TestApp
              window "Main"
                button @e1 "Submit"
                button @e2 "Cancel"
                link @e3 "Help"
            """
        let diff = differ.diff(old: old, new: new)
        #expect(diff.removed.count == 1)  // Reset
        #expect(diff.added.count == 2)  // Cancel + Help
        #expect(diff.removed[0].text.contains("Reset"))
    }

    // MARK: - Empty snapshots

    @Test("empty old snapshot treats everything as added")
    func emptyOld() {
        let old = "app: TestApp"
        let new = """
            app: TestApp
              button @e1 "OK"
            """
        let diff = differ.diff(old: old, new: new)
        #expect(diff.added.count == 1)
        #expect(diff.removed.isEmpty)
    }

    @Test("empty new snapshot treats everything as removed")
    func emptyNew() {
        let old = """
            app: TestApp
              button @e1 "OK"
            """
        let new = "app: TestApp"
        let diff = differ.diff(old: old, new: new)
        #expect(diff.removed.count == 1)
        #expect(diff.added.isEmpty)
    }

    // MARK: - Rendering

    @Test("render shows +/- markers")
    func renderMarkers() {
        let old = """
            app: TestApp
              button @e1 "OK"
            """
        let new = """
            app: TestApp
              button @e1 "OK"
              button @e2 "Cancel"
            """
        let diff = differ.diff(old: old, new: new)
        let output = diff.render()
        #expect(output.contains("+"))
        #expect(output.contains("Cancel"))
        #expect(output.contains("[diff:"))
    }

    @Test("render with context shows surrounding unchanged lines")
    func renderWithContext() {
        let old = """
            app: TestApp
              window "Main"
                group "A"
                  button @e1 "One"
                group "B"
                  button @e2 "Two"
                group "C"
                  button @e3 "Three"
            """
        let new = """
            app: TestApp
              window "Main"
                group "A"
                  button @e1 "One"
                group "B"
                  button @e2 "Two"
                  button @e3 "New"
                group "C"
                  button @e4 "Three"
            """
        let diff = differ.diff(old: old, new: new)
        let output = diff.render(context: 1)
        // Should show context lines around the addition
        #expect(output.contains("  "))  // unchanged lines have 2-space prefix
        #expect(output.contains("+ "))  // added line
    }

    @Test("no changes renders [no changes]")
    func renderNoChanges() {
        let text = """
            app: TestApp
              button @e1 "OK"
            """
        let diff = differ.diff(old: text, new: text)
        let output = diff.render()
        #expect(output == "[no changes]")
    }

    // MARK: - Summary

    @Test("summary describes changes")
    func summaryDescribes() {
        let old = """
            app: TestApp
              button @e1 "A"
              button @e2 "B"
            """
        let new = """
            app: TestApp
              button @e1 "A"
              button @e2 "C"
              button @e3 "D"
            """
        let diff = differ.diff(old: old, new: new)
        #expect(diff.summary.contains("added"))
        #expect(diff.summary.contains("removed"))
    }

    // MARK: - Structural changes (indentation matters)

    @Test("element moved to different parent shows as remove+add")
    func elementMoved() {
        let old = """
            app: TestApp
              group "A"
                button @e1 "OK"
              group "B"
            """
        let new = """
            app: TestApp
              group "A"
              group "B"
                button @e1 "OK"
            """
        let diff = differ.diff(old: old, new: new)
        // Button moved from group A to group B -- different indentation context
        #expect(diff.added.count == 1)
        #expect(diff.removed.count == 1)
    }

    // MARK: - Snapshot cache

    @Test("cache roundtrip saves and loads text")
    func cacheRoundtrip() throws {
        let cache = SnapshotCache()
        let app = "TestApp-\(Int.random(in: 1000...9999))"
        defer { cache.clear(app: app) }

        let text = "app: \(app)\n  button @e1 \"OK\""
        try cache.save(app: app, text: text)

        let loaded = cache.load(app: app)
        #expect(loaded == text)
    }

    @Test("cache returns nil for unknown app")
    func cacheReturnsNil() {
        let cache = SnapshotCache()
        #expect(cache.load(app: "NonexistentApp-99999") == nil)
    }
}
