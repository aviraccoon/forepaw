import ArgumentParser
import ForepawCore
import ForepawDarwin
import Foundation

struct Click: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Click an element by ref"
    )

    @Argument(help: "Element ref (e.g. @e3)")
    var ref: String

    @Option(name: .long, help: "Target application name")
    var app: String

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        guard let elementRef = ElementRef.parse(ref) else {
            throw ValidationError("Invalid ref: \(ref). Expected format: @e1, @e2, etc.")
        }
        let provider = DarwinProvider()
        let result = try await provider.click(ref: elementRef, app: app)
        let formatter = OutputFormatter(json: json)
        print(formatter.format(success: result.success, command: "click", data: ["text": result.message ?? "clicked"]))
    }
}

struct Type: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Type text into an element"
    )

    @Argument(help: "Element ref (e.g. @e5)")
    var ref: String

    @Argument(help: "Text to type")
    var text: String

    @Option(name: .long, help: "Target application name")
    var app: String

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        guard let elementRef = ElementRef.parse(ref) else {
            throw ValidationError("Invalid ref: \(ref). Expected format: @e1, @e2, etc.")
        }
        let provider = DarwinProvider()
        let result = try await provider.type(ref: elementRef, text: text, app: app)
        let formatter = OutputFormatter(json: json)
        print(formatter.format(success: result.success, command: "type", data: ["text": result.message ?? "typed"]))
    }
}

struct KeyboardType: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "keyboard-type",
        abstract: "Type text into the focused element (no ref needed)"
    )

    @Argument(help: "Text to type")
    var text: String

    @Option(name: .long, help: "Target application name (activates app first; omit to type into current focus)")
    var app: String?

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let provider = DarwinProvider()
        let result: ActionResult
        if let app {
            result = try await provider.keyboardType(text: text, app: app)
        } else {
            result = try await provider.keyboardType(text: text)
        }
        let formatter = OutputFormatter(json: json)
        print(
            formatter.format(
                success: result.success, command: "keyboard-type", data: ["text": result.message ?? "typed"]))
    }
}

struct Press: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Press a keyboard shortcut (e.g. cmd+s, ctrl+shift+z)"
    )

    @Argument(help: "Key combo (e.g. cmd+s, return, escape)")
    var combo: String

    @Option(name: .long, help: "Target application name (activates app first; omit for global hotkeys)")
    var app: String?

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let keyCombo = KeyCombo.parse(combo)
        let provider = DarwinProvider()
        let result: ActionResult
        if let app {
            result = try await provider.press(keys: keyCombo, app: app)
        } else {
            result = try await provider.press(keys: keyCombo)
        }
        let formatter = OutputFormatter(json: json)
        print(formatter.format(success: result.success, command: "press", data: ["text": result.message ?? "pressed"]))
    }
}

struct OCRClick: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "ocr-click",
        abstract: "Find text on screen via OCR and click it"
    )

    @Argument(help: "Text to find and click")
    var text: String

    @Option(name: .long, help: "Target application name")
    var app: String

    @Option(name: .long, help: "Window title or ID (e.g. 'Hacker News' or 'w-7290')")
    var window: String?

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let provider = DarwinProvider()
        let result = try await provider.ocrClick(text: text, app: app, window: window)
        let formatter = OutputFormatter(json: json)
        print(
            formatter.format(success: result.success, command: "ocr-click", data: ["text": result.message ?? "clicked"])
        )
    }
}

struct Scroll: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Scroll within an app window"
    )

    @Argument(help: "Direction: up, down, left, right")
    var direction: String

    @Option(name: .long, help: "Target application name")
    var app: String

    @Option(name: .long, help: "Window title or ID (e.g. 'Hacker News' or 'w-7290')")
    var window: String?

    @Option(name: .shortAndLong, help: "Number of scroll ticks (default 3)")
    var amount: Int = 3

    @Option(name: .long, help: "Element ref to scroll within (e.g. @e5)")
    var ref: String?

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let validDirections = ["up", "down", "left", "right"]
        guard validDirections.contains(direction) else {
            throw ValidationError("Invalid direction '\(direction)'. Use: \(validDirections.joined(separator: ", "))")
        }

        var elementRef: ElementRef?
        if let refStr = ref {
            guard let parsed = ElementRef.parse(refStr) else {
                throw ValidationError("Invalid ref: \(refStr). Expected format: @e1, @e2, etc.")
            }
            elementRef = parsed
        }

        let provider = DarwinProvider()
        let result = try await provider.scroll(
            direction: direction, amount: amount, app: app, window: window, ref: elementRef)
        let formatter = OutputFormatter(json: json)
        print(
            formatter.format(success: result.success, command: "scroll", data: ["text": result.message ?? "scrolled"]))
    }
}
