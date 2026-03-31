import Foundation
import Testing

@testable import ForepawCore

@Suite("DragOptions")
struct DragOptionsTests {
    @Test("default values")
    func defaults() {
        let opts = DragOptions()
        #expect(opts.steps == 30)
        #expect(opts.duration == 0.3)
        #expect(opts.modifiers.isEmpty)
        #expect(opts.pressure == nil)
        #expect(opts.rightButton == false)
        #expect(opts.closePath == false)
    }

    @Test("custom values")
    func custom() {
        let opts = DragOptions(
            steps: 60, duration: 1.5,
            modifiers: [.shift, .option], pressure: 0.7,
            rightButton: true, closePath: true)
        #expect(opts.steps == 60)
        #expect(opts.duration == 1.5)
        #expect(opts.modifiers == [.shift, .option])
        #expect(opts.pressure == 0.7)
        #expect(opts.rightButton == true)
        #expect(opts.closePath == true)
    }
}

@Suite("Point")
struct PointTests {
    @Test("init and fields")
    func basic() {
        let p = Point(x: 100.5, y: 200.75)
        #expect(p.x == 100.5)
        #expect(p.y == 200.75)
    }

    @Test("Codable round-trip")
    func codable() throws {
        let original = Point(x: 42.0, y: 99.5)
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(Point.self, from: data)
        #expect(decoded.x == original.x)
        #expect(decoded.y == original.y)
    }
}

@Suite("parseModifiers")
struct ParseModifiersTests {
    @Test("nil input returns empty")
    func nilInput() {
        #expect(KeyCombo.Modifier.parseModifiers(nil).isEmpty)
    }

    @Test("empty string returns empty")
    func emptyString() {
        #expect(KeyCombo.Modifier.parseModifiers("").isEmpty)
    }

    @Test("single modifier")
    func single() {
        #expect(KeyCombo.Modifier.parseModifiers("shift") == [.shift])
        #expect(KeyCombo.Modifier.parseModifiers("cmd") == [.command])
        #expect(KeyCombo.Modifier.parseModifiers("alt") == [.option])
        #expect(KeyCombo.Modifier.parseModifiers("ctrl") == [.control])
    }

    @Test("modifier aliases")
    func aliases() {
        #expect(KeyCombo.Modifier.parseModifiers("opt") == [.option])
        #expect(KeyCombo.Modifier.parseModifiers("option") == [.option])
        #expect(KeyCombo.Modifier.parseModifiers("command") == [.command])
        #expect(KeyCombo.Modifier.parseModifiers("control") == [.control])
    }

    @Test("combined modifiers")
    func combined() {
        let mods = KeyCombo.Modifier.parseModifiers("shift+alt")
        #expect(mods.count == 2)
        #expect(mods.contains(.shift))
        #expect(mods.contains(.option))
    }

    @Test("all four modifiers")
    func allFour() {
        let mods = KeyCombo.Modifier.parseModifiers("cmd+shift+opt+ctrl")
        #expect(mods.count == 4)
        #expect(mods.contains(.command))
        #expect(mods.contains(.shift))
        #expect(mods.contains(.option))
        #expect(mods.contains(.control))
    }

    @Test("case insensitive")
    func caseInsensitive() {
        let mods = KeyCombo.Modifier.parseModifiers("Shift+CMD")
        #expect(mods.count == 2)
        #expect(mods.contains(.shift))
        #expect(mods.contains(.command))
    }

    @Test("unknown tokens are skipped")
    func unknownSkipped() {
        let mods = KeyCombo.Modifier.parseModifiers("shift+banana+ctrl")
        #expect(mods.count == 2)
        #expect(mods.contains(.shift))
        #expect(mods.contains(.control))
    }
}
