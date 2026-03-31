import ArgumentParser
import ForepawCore
import ForepawDarwin
import Foundation

@main
struct Forepaw: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "forepaw",
        abstract: "A raccoon's paws on your UI. Desktop automation for AI agents. (\(forepawVersion))",
        version: forepawVersion,
        subcommands: [
            Snapshot.self,
            Click.self,
            Type.self,
            KeyboardType.self,
            Press.self,
            Screenshot.self,
            ListApps.self,
            ListWindows.self,
            OCR.self,
            OCRClick.self,
            Scroll.self,
            Permissions.self,
        ]
    )
}

// MARK: - Shared options

struct GlobalOptions: ParsableArguments {
    @Option(name: .long, help: "Target application name")
    var app: String?

    @Option(name: .long, help: "Window title or ID (e.g. 'Hacker News' or 'w-7290')")
    var window: String?

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false
}

// MARK: - Commands

struct Snapshot: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Accessibility tree with element refs"
    )

    @OptionGroup var global: GlobalOptions

    @Flag(name: .shortAndLong, help: "Only interactive elements")
    var interactive: Bool = false

    @Flag(name: .shortAndLong, help: "Remove empty structural elements")
    var compact: Bool = false

    @Option(name: .shortAndLong, help: "Maximum tree depth")
    var depth: Int = 15

    mutating func run() async throws {
        guard let app = global.app else {
            throw ValidationError("--app is required")
        }
        let provider = DarwinProvider()
        let options = SnapshotOptions(
            interactiveOnly: interactive,
            maxDepth: depth,
            compact: compact
        )
        let tree = try await provider.snapshot(app: app, options: options)
        let renderer = TreeRenderer()
        print(renderer.render(tree: tree))
    }
}

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

struct Screenshot: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Take a screenshot"
    )

    @OptionGroup var global: GlobalOptions

    @Flag(name: .long, help: "Annotate with numbered labels")
    var annotate: Bool = false

    mutating func run() async throws {
        let provider = DarwinProvider()
        let result = try await provider.screenshot(app: global.app, window: global.window, annotate: annotate)
        print(result.path)
        if let legend = result.legend {
            print(legend)
        }
    }
}

struct ListApps: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-apps",
        abstract: "List running GUI applications"
    )

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let provider = DarwinProvider()
        let apps = try await provider.listApps()
        if json {
            let encoder = JSONEncoder()
            encoder.outputFormatting = .prettyPrinted
            let data = try encoder.encode(apps)
            print(String(data: data, encoding: .utf8) ?? "[]")
        } else {
            for app in apps.sorted(by: { $0.name < $1.name }) {
                let bundle = app.bundleID.map { " (\($0))" } ?? ""
                print("\(app.name)\(bundle) [pid: \(app.pid)]")
            }
        }
    }
}

struct ListWindows: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-windows",
        abstract: "List visible windows"
    )

    @OptionGroup var global: GlobalOptions

    mutating func run() async throws {
        let provider = DarwinProvider()
        let windows = try await provider.listWindows(app: global.app)
        for window in windows {
            print("\(window.id)  \(window.app)  \"\(window.title)\"")
        }
    }
}

struct OCR: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "ocr",
        abstract: "Screenshot and run OCR, returning recognized text with coordinates"
    )

    @OptionGroup var global: GlobalOptions

    @Option(name: .long, help: "Filter results containing this text")
    var find: String?

    mutating func run() async throws {
        let provider = DarwinProvider()
        let results = try await provider.ocr(app: global.app, window: global.window, find: find)

        if global.json {
            for r in results {
                print(
                    "{\"text\": \"\(r.text)\", \"x\": \(Int(r.center.x)), \"y\": \(Int(r.center.y)), \"bounds\": {\"x\": \(Int(r.bounds.x)), \"y\": \(Int(r.bounds.y)), \"w\": \(Int(r.bounds.width)), \"h\": \(Int(r.bounds.height))}}"
                )
            }
        } else {
            for r in results {
                print("\(r.text)  [\(Int(r.center.x)),\(Int(r.center.y))]")
            }
        }

        if results.isEmpty {
            print(find != nil ? "No text matching '\(find!)' found" : "No text recognized")
        }
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

struct Permissions: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Check or request accessibility permissions"
    )

    @Flag(name: .long, help: "Prompt for permission")
    var request: Bool = false

    static let accessibilityHelp = """

        To grant accessibility permission:
          1. Open System Settings > Privacy & Security > Accessibility
          2. Click the + button
          3. Add your terminal app (Terminal, Ghostty, Warp, iTerm2, etc.)
          4. Ensure the toggle is enabled
        """

    static let screenRecordingHelp = """

        To grant screen recording permission:
          1. Open System Settings > Privacy & Security > Screen & System Audio Recording
          2. Click the + button
          3. Add your terminal app
          4. Ensure the toggle is enabled
        """

    mutating func run() async throws {
        let provider = DarwinProvider()
        var failed = false

        if request {
            let axGranted = provider.requestPermissions()
            let srGranted = provider.requestScreenRecordingPermission()
            if axGranted {
                print("Accessibility: granted")
            } else {
                print("Accessibility: not granted")
                print(Self.accessibilityHelp)
                failed = true
            }
            if srGranted {
                print("Screen recording: granted")
            } else {
                print("Screen recording: not granted")
                print(Self.screenRecordingHelp)
                failed = true
            }
        } else {
            let hasAX = provider.hasPermissions()
            let hasSR = provider.hasScreenRecordingPermission()
            if hasAX {
                print("Accessibility: granted")
            } else {
                print("Accessibility: not granted")
                print(Self.accessibilityHelp)
                failed = true
            }
            if hasSR {
                print("Screen recording: granted")
            } else {
                print("Screen recording: not granted")
                print(Self.screenRecordingHelp)
                failed = true
            }
        }

        if failed { throw ExitCode.failure }
    }
}
