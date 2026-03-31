import Testing

@testable import ForepawCore

@Suite("KeyCombo")
struct KeyComboTests {
    @Test("parse simple key")
    func simpleKey() {
        let combo = KeyCombo.parse("return")
        #expect(combo.key == "return")
        #expect(combo.modifiers.isEmpty)
    }

    @Test("parse single modifier + key")
    func singleModifier() {
        let combo = KeyCombo.parse("cmd+s")
        #expect(combo.key == "s")
        #expect(combo.modifiers == [.command])
    }

    @Test("parse multiple modifiers")
    func multipleModifiers() {
        let combo = KeyCombo.parse("cmd+shift+s")
        #expect(combo.key == "s")
        #expect(combo.modifiers.contains(.command))
        #expect(combo.modifiers.contains(.shift))
        #expect(combo.modifiers.count == 2)
    }

    @Test("parse all modifier aliases")
    func modifierAliases() {
        // cmd/command/meta/super
        #expect(KeyCombo.parse("cmd+a").modifiers == [.command])
        #expect(KeyCombo.parse("command+a").modifiers == [.command])
        #expect(KeyCombo.parse("meta+a").modifiers == [.command])
        #expect(KeyCombo.parse("super+a").modifiers == [.command])

        // opt/option/alt
        #expect(KeyCombo.parse("opt+a").modifiers == [.option])
        #expect(KeyCombo.parse("option+a").modifiers == [.option])
        #expect(KeyCombo.parse("alt+a").modifiers == [.option])

        // ctrl/control
        #expect(KeyCombo.parse("ctrl+a").modifiers == [.control])
        #expect(KeyCombo.parse("control+a").modifiers == [.control])

        // shift
        #expect(KeyCombo.parse("shift+a").modifiers == [.shift])
    }

    @Test("parse is case-insensitive")
    func caseInsensitive() {
        let combo = KeyCombo.parse("CMD+Shift+S")
        #expect(combo.key == "s")
        #expect(combo.modifiers.contains(.command))
        #expect(combo.modifiers.contains(.shift))
    }

    @Test("parse escape and special keys")
    func specialKeys() {
        #expect(KeyCombo.parse("escape").key == "escape")
        #expect(KeyCombo.parse("tab").key == "tab")
        #expect(KeyCombo.parse("space").key == "space")
        #expect(KeyCombo.parse("delete").key == "delete")
    }

    @Test("parse four modifiers")
    func fourModifiers() {
        let combo = KeyCombo.parse("cmd+shift+opt+ctrl+z")
        #expect(combo.key == "z")
        #expect(combo.modifiers.count == 4)
        #expect(combo.modifiers.contains(.command))
        #expect(combo.modifiers.contains(.shift))
        #expect(combo.modifiers.contains(.option))
        #expect(combo.modifiers.contains(.control))
    }
}
