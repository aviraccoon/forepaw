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

    @Flag(name: .long, help: "Show diff against previous snapshot of this app")
    var diff: Bool = false

    @Option(name: .long, help: "Context lines around changes in diff output (default: 0)")
    var context: Int = 0

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
        let rendered = renderer.render(tree: tree)

        let cache = SnapshotCache()

        if diff {
            if let previous = cache.load(app: app) {
                let differ = SnapshotDiffer()
                let result = differ.diff(old: previous, new: rendered)
                print(result.render(context: context))
            } else {
                // No previous snapshot -- show the full tree as all "added"
                print("[diff: no previous snapshot cached for \(app)]")
                print(rendered)
            }
        } else {
            print(rendered)
        }

        // Always cache the current snapshot for future diffs
        try? cache.save(app: app, text: rendered)
    }
}

struct Screenshot: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Take a screenshot"
    )

    @OptionGroup var global: GlobalOptions

    @Flag(name: .long, help: "Annotate with numbered labels (shorthand for --style badges)")
    var annotate: Bool = false

    @Option(
        name: .long,
        help: "Annotation style: badges (compact), labeled (with roles/names), spotlight (dim non-interactive)")
    var style: String?

    @Option(
        name: .long, parsing: .upToNextOption,
        help: "Only annotate these refs (e.g. --only @e5 @e8 @e12)")
    var only: [String] = []

    @Option(name: .long, help: "Image format: jpeg, png, or webp (default: best available)")
    var format: String?

    @Option(name: .long, help: "JPEG quality 1-100 (default 85)")
    var quality: Int?

    @Option(name: .long, help: "Output scale: 1 (default, logical pixels) or 2 (Retina)")
    var scale: Int?

    @Flag(name: .long, help: "Exclude mouse cursor from screenshot")
    var noCursor: Bool = false

    mutating func run() async throws {
        let annotationStyle = resolveAnnotationStyle()
        let refFilter = only.isEmpty ? nil : only.compactMap { ElementRef.parse($0) }
        let ssOptions = buildScreenshotOptions()
        let result = try await provider.screenshot(
            app: global.app, window: global.window,
            style: annotationStyle, only: refFilter,
            options: ssOptions)
        print(result.path)
        if let legend = result.legend {
            print(legend)
        }
    }

    private func buildScreenshotOptions() -> ScreenshotOptions {
        let fmt: ImageFormat
        if let f = format {
            fmt = ImageFormat(rawValue: f) ?? .bestAvailable
        } else {
            fmt = .bestAvailable
        }
        return ScreenshotOptions(
            format: fmt,
            quality: quality ?? 85,
            scale: scale ?? 1,
            cursor: !noCursor
        )
    }

    private func resolveAnnotationStyle() -> AnnotationStyle? {
        if let style = style {
            guard let parsed = AnnotationStyle(rawValue: style) else {
                // Will print error and exit
                print(
                    "error: unknown style '\(style)'. Options: \(AnnotationStyle.allCases.map(\.rawValue).joined(separator: ", "))"
                )
                return nil
            }
            return parsed
        }
        if annotate {
            return .badges
        }
        return nil
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

    @Flag(name: .long, help: "Skip saving the display screenshot (only output OCR text)")
    var noScreenshot: Bool = false

    @Option(name: .long, help: "Image format for screenshot: jpeg, png, or webp (default: best available)")
    var format: String?

    @Option(name: .long, help: "JPEG quality 1-100 (default 85)")
    var quality: Int?

    @Option(name: .long, help: "Output scale: 1 (default, logical pixels) or 2 (Retina)")
    var scale: Int?

    @Flag(name: .long, help: "Exclude mouse cursor from screenshot")
    var noCursor: Bool = false

    mutating func run() async throws {
        let ssOptions: ScreenshotOptions? = noScreenshot ? nil : buildScreenshotOptions()
        let output = try await provider.ocr(
            app: global.app, window: global.window, find: find,
            screenshotOptions: ssOptions)

        // Print screenshot path first (most useful to agents)
        if let path = output.screenshotPath {
            print(path)
        }

        if global.json {
            for r in output.results {
                print(
                    "{\"text\": \"\(r.text)\", \"x\": \(Int(r.center.x)), \"y\": \(Int(r.center.y)), \"bounds\": {\"x\": \(Int(r.bounds.x)), \"y\": \(Int(r.bounds.y)), \"w\": \(Int(r.bounds.width)), \"h\": \(Int(r.bounds.height))}}"
                )
            }
        } else {
            for r in output.results {
                print("\(r.text)  [\(Int(r.center.x)),\(Int(r.center.y))]")
            }
        }

        if output.results.isEmpty {
            print(find != nil ? "No text matching '\(find!)' found" : "No text recognized")
        }
    }

    private func buildScreenshotOptions() -> ScreenshotOptions {
        let fmt: ImageFormat
        if let f = format {
            fmt = ImageFormat(rawValue: f) ?? .bestAvailable
        } else {
            fmt = .bestAvailable
        }
        return ScreenshotOptions(
            format: fmt,
            quality: quality ?? 85,
            scale: scale ?? 1,
            cursor: !noCursor
        )
    }
}
