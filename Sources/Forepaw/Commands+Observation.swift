import ArgumentParser
import ForepawCore
import Foundation

struct Snapshot: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Accessibility tree with element refs"
    )

    @OptionGroup var global: GlobalOptions

    @Flag(name: .shortAndLong, help: "Only interactive elements")
    var interactive: Bool = false

    @Flag(name: .shortAndLong, help: "Remove empty structural elements")
    var compact: Bool = false

    @Option(name: .shortAndLong, help: "Maximum tree depth (default 15)")
    var depth: Int = SnapshotOptions.defaultDepth

    mutating func run() async throws {
        guard let app = global.app else {
            throw ValidationError("--app is required")
        }
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

struct Screenshot: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Take a screenshot"
    )

    @OptionGroup var global: GlobalOptions

    @Flag(name: .long, help: "Annotate with numbered labels")
    var annotate: Bool = false

    mutating func run() async throws {
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
