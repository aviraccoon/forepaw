import ArgumentParser
import ForepawCore
import Foundation

struct Click: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Click an element by ref or at coordinates"
    )

    @Argument(help: "Element ref (e.g. @e3) or window-relative coordinates (e.g. 500,300)")
    var target: String

    @Option(name: .long, help: "Target application name (required; coordinates are relative to window)")
    var app: String?

    @Flag(name: .long, help: "Right-click (context menu)")
    var right: Bool = false

    @Flag(name: .long, help: "Double-click")
    var double: Bool = false

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let options = ClickOptions(
            button: right ? .right : .left,
            clickCount: double ? 2 : 1
        )
        let result: ActionResult

        if let elementRef = ElementRef.parse(target) {
            guard let app else {
                throw ValidationError("--app is required for ref-based click")
            }
            result = try await provider.click(ref: elementRef, app: app, options: options)
        } else if let point = parseCoordinate(target) {
            guard let app else {
                throw ValidationError("--app is required for coordinate-based click")
            }
            result = try await provider.clickAtPoint(point, app: app, options: options)
        } else {
            throw ValidationError(
                "Invalid target: \(target). Expected a ref (@e1) or coordinates (500,300).")
        }

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

    @Argument(help: ArgumentHelp("Text to type", valueName: "text"))
    var positionalText: String?

    @Option(
        name: .customLong("text"), parsing: .unconditional,
        help: "Text to type (use instead of positional for text starting with dashes)")
    var textOption: String?

    @Option(name: .long, help: "Target application name")
    var app: String

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let text = try resolveText(positional: positionalText, option: textOption, command: "type")
        guard let elementRef = ElementRef.parse(ref) else {
            throw ValidationError("Invalid ref: \(ref). Expected format: @e1, @e2, etc.")
        }
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

    @Argument(help: ArgumentHelp("Text to type", valueName: "text"))
    var positionalText: String?

    @Option(
        name: .customLong("text"), parsing: .unconditional,
        help: "Text to type (use instead of positional for text starting with dashes)")
    var textOption: String?

    @Option(name: .long, help: "Target application name (activates app first; omit to type into current focus)")
    var app: String?

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let text = try resolveText(positional: positionalText, option: textOption, command: "keyboard-type")
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

    @Argument(help: ArgumentHelp("Text to find and click", valueName: "text"))
    var positionalText: String?

    @Option(
        name: .customLong("text"), parsing: .unconditional,
        help: "Text to find and click (use instead of positional for text starting with dashes)")
    var textOption: String?

    @Option(name: .long, help: "Target application name")
    var app: String

    @Option(name: .long, help: "Window title or ID (e.g. 'Hacker News' or 'w-7290')")
    var window: String?

    @Flag(name: .long, help: "Right-click (context menu)")
    var right: Bool = false

    @Flag(name: .long, help: "Double-click")
    var double: Bool = false

    @Option(name: .long, help: "Which match to click (1-based) when multiple found")
    var index: Int?

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let text = try resolveText(positional: positionalText, option: textOption, command: "ocr-click")
        let options = ClickOptions(
            button: right ? .right : .left,
            clickCount: double ? 2 : 1
        )
        let result = try await provider.ocrClick(
            text: text, app: app, window: window, options: options, index: index)
        let formatter = OutputFormatter(json: json)
        print(
            formatter.format(success: result.success, command: "ocr-click", data: ["text": result.message ?? "clicked"])
        )
    }
}

struct Hover: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Move mouse to an element without clicking (triggers tooltips/hover states)"
    )

    @Argument(help: "Element ref (e.g. @e3), text for OCR, or window-relative coords (e.g. 500,300)")
    var target: String

    @Option(name: .long, help: "Target application name (coordinates are window-relative when set)")
    var app: String?

    @Option(name: .long, help: "Window title or ID (e.g. 'Hacker News' or 'w-7290')")
    var window: String?

    @Flag(name: .long, help: "Smooth mouse movement (triggers mouseEnter/mouseLeave events)")
    var smooth: Bool = false

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let result: ActionResult

        if let elementRef = ElementRef.parse(target) {
            guard let app else {
                throw ValidationError("--app is required for ref-based hover")
            }
            result = try await provider.hover(ref: elementRef, app: app)
        } else if let point = parseCoordinate(target) {
            result = try await provider.hoverAtPoint(point, app: app, smooth: smooth)
        } else {
            guard let app else {
                throw ValidationError("--app is required for text-based hover")
            }
            result = try await provider.ocrHover(text: target, app: app, window: window, index: nil)
        }

        let formatter = OutputFormatter(json: json)
        print(formatter.format(success: result.success, command: "hover", data: ["text": result.message ?? "hovered"]))
    }
}

struct Wait: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Wait for text to appear on screen (OCR polling)"
    )

    @Argument(help: ArgumentHelp("Text to wait for", valueName: "text"))
    var positionalText: String?

    @Option(
        name: .customLong("text"), parsing: .unconditional,
        help: "Text to wait for (use instead of positional for text starting with dashes)")
    var textOption: String?

    @Option(name: .long, help: "Target application name")
    var app: String

    @Option(name: .long, help: "Window title or ID (e.g. 'Hacker News' or 'w-7290')")
    var window: String?

    @Option(name: .long, help: "Maximum seconds to wait (default 10)")
    var timeout: Double = 10

    @Option(name: .long, help: "Seconds between polls (default 1)")
    var interval: Double = 1

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let text = try resolveText(positional: positionalText, option: textOption, command: "wait")
        let result = try await provider.wait(
            text: text, app: app, window: window,
            timeout: timeout, interval: interval)
        let formatter = OutputFormatter(json: json)
        print(formatter.format(success: result.success, command: "wait", data: ["text": result.message ?? "found"]))
    }
}

struct Batch: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Execute multiple actions in one invocation"
    )

    @Argument(
        parsing: .remaining, help: "Actions separated by ;; (e.g. 'click @e3 ;; press cmd+s ;; keyboard-type hello')")
    var args: [String]

    @Option(name: .long, help: "Target application name (applies to all actions unless overridden)")
    var app: String?

    @Option(name: .long, help: "Window title or ID")
    var window: String?

    @Option(name: .long, help: "Delay in milliseconds between actions (default 100)")
    var delay: Int = 100

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        // Join all args and split on ;;
        let joined = args.joined(separator: " ")
        let actions = joined.components(separatedBy: ";;")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }

        guard !actions.isEmpty else {
            throw ValidationError("No actions provided. Separate actions with ;;")
        }

        let formatter = OutputFormatter(json: json)
        var results: [(String, ActionResult)] = []

        for (i, action) in actions.enumerated() {
            let result = try await executeAction(action, provider: provider)
            results.append((action, result))
            print(
                formatter.format(
                    success: result.success, command: action,
                    data: ["text": result.message ?? "ok"]))

            if i < actions.count - 1 {
                try await Task.sleep(nanoseconds: UInt64(delay) * 1_000_000)
            }
        }
    }

    private func executeAction(_ action: String, provider: any DesktopProvider) async throws -> ActionResult {
        let parts = shellSplit(action)
        guard let command = parts.first else {
            throw ForepawError.actionFailed("Empty action")
        }
        let actionArgs = Array(parts.dropFirst())

        switch command {
        case "click":
            guard let target = actionArgs.first else {
                throw ForepawError.actionFailed("click requires a ref or coordinates (e.g. click @e3 or click 500,300)")
            }
            let appName = parseOption("--app", from: actionArgs) ?? app
            let options = ClickOptions(
                button: actionArgs.contains("--right") ? .right : .left,
                clickCount: actionArgs.contains("--double") ? 2 : 1
            )
            if let ref = ElementRef.parse(target) {
                guard let appName else {
                    throw ForepawError.actionFailed("click requires --app (on action or batch)")
                }
                return try await provider.click(ref: ref, app: appName, options: options)
            } else if let point = parseCoordinate(target) {
                guard let appName else {
                    throw ForepawError.actionFailed("click requires --app (on action or batch)")
                }
                return try await provider.clickAtPoint(point, app: appName, options: options)
            } else {
                throw ForepawError.actionFailed(
                    "Invalid click target: \(target). Use a ref (@e3) or coordinates (500,300)")
            }

        case "hover":
            guard let target = actionArgs.first else {
                throw ForepawError.actionFailed(
                    "hover requires a ref, text, or coordinates (e.g. hover @e3 or hover 500,300)")
            }
            let appName = parseOption("--app", from: actionArgs) ?? app
            if let ref = ElementRef.parse(target) {
                guard let appName else {
                    throw ForepawError.actionFailed("hover requires --app (on action or batch)")
                }
                return try await provider.hover(ref: ref, app: appName)
            } else if let point = parseCoordinate(target) {
                let smoothMove = actionArgs.contains("--smooth")
                return try await provider.hoverAtPoint(point, app: appName, smooth: smoothMove)
            } else {
                guard let appName else {
                    throw ForepawError.actionFailed("hover requires --app (on action or batch)")
                }
                let win = parseOption("--window", from: actionArgs) ?? window
                return try await provider.ocrHover(text: target, app: appName, window: win, index: nil)
            }

        case "type":
            guard let refStr = actionArgs.first,
                let ref = ElementRef.parse(refStr)
            else {
                throw ForepawError.actionFailed("type requires ref and text (e.g. type @e3 hello)")
            }
            let text =
                parseOption("--text", from: actionArgs)
                ?? collectPositionalText(from: actionArgs, skip: 1)
            guard let text else {
                throw ForepawError.actionFailed("type requires text (e.g. type @e3 hello or type @e3 --text hello)")
            }
            let appName = parseOption("--app", from: actionArgs) ?? app
            guard let appName else {
                throw ForepawError.actionFailed("type requires --app (on action or batch)")
            }
            return try await provider.type(ref: ref, text: text, app: appName)

        case "keyboard-type":
            let text =
                parseOption("--text", from: actionArgs)
                ?? collectPositionalText(from: actionArgs, skip: 0)
            guard let text else {
                throw ForepawError.actionFailed("keyboard-type requires text")
            }
            let appName = parseOption("--app", from: actionArgs) ?? app
            if let appName {
                return try await provider.keyboardType(text: text, app: appName)
            }
            return try await provider.keyboardType(text: text)

        case "press":
            guard let comboStr = actionArgs.first else {
                throw ForepawError.actionFailed("press requires a key combo (e.g. press cmd+s)")
            }
            let keyCombo = KeyCombo.parse(comboStr)
            let appName = parseOption("--app", from: actionArgs) ?? app
            if let appName {
                return try await provider.press(keys: keyCombo, app: appName)
            }
            return try await provider.press(keys: keyCombo)

        case "scroll":
            guard let direction = actionArgs.first else {
                throw ForepawError.actionFailed("scroll requires a direction (up/down/left/right)")
            }
            let appName = parseOption("--app", from: actionArgs) ?? app
            guard let appName else {
                throw ForepawError.actionFailed("scroll requires --app (on action or batch)")
            }
            let amount = parseOption("--amount", from: actionArgs).flatMap { Int($0) } ?? 3
            let win = parseOption("--window", from: actionArgs) ?? window
            let scrollRef = parseOption("--ref", from: actionArgs).flatMap { ElementRef.parse($0) }
            let scrollAt = parseOption("--at", from: actionArgs).flatMap { parseCoordinate($0) }
            return try await provider.scroll(
                direction: direction, amount: amount, app: appName, window: win,
                ref: scrollRef, at: scrollAt)

        case "ocr-click":
            let text = parseOption("--text", from: actionArgs) ?? actionArgs.first
            guard let text else {
                throw ForepawError.actionFailed("ocr-click requires text")
            }
            let appName = parseOption("--app", from: actionArgs) ?? app
            guard let appName else {
                throw ForepawError.actionFailed("ocr-click requires --app (on action or batch)")
            }
            let win = parseOption("--window", from: actionArgs) ?? window
            let options = ClickOptions(
                button: actionArgs.contains("--right") ? .right : .left,
                clickCount: actionArgs.contains("--double") ? 2 : 1
            )
            let ocrIndex = parseOption("--index", from: actionArgs).flatMap { Int($0) }
            return try await provider.ocrClick(
                text: text, app: appName, window: win, options: options, index: ocrIndex)

        case "drag":
            guard actionArgs.count >= 2 else {
                throw ForepawError.actionFailed(
                    "drag requires at least 2 targets (e.g. drag 100,100 500,500)")
            }
            let appName = parseOption("--app", from: actionArgs) ?? app
            let dragSteps = parseOption("--steps", from: actionArgs).flatMap { Int($0) } ?? 30
            let dragDuration =
                parseOption("--duration", from: actionArgs).flatMap { Double($0) } ?? 0.3
            let dragPressure =
                parseOption("--pressure", from: actionArgs).flatMap { Double($0) }

            // Parse modifier and behavior flags
            let dragModifiers = KeyCombo.Modifier.parseModifiers(
                parseOption("--modifiers", from: actionArgs))
            let dragRight = actionArgs.contains("--right")
            let dragClose = actionArgs.contains("--close")

            let dragOptions = DragOptions(
                steps: dragSteps, duration: dragDuration,
                modifiers: dragModifiers, pressure: dragPressure,
                rightButton: dragRight, closePath: dragClose)

            // Collect coordinate/ref targets (filter out flags and option values)
            let knownFlags: Set<String> = [
                "--right", "--close",
            ]
            let dragTargets = actionArgs.filter {
                !$0.starts(with: "--") && !knownFlags.contains($0)
                    && parseOption($0, from: actionArgs) == nil
            }
            .prefix(while: { parseCoordinate($0) != nil || ElementRef.parse($0) != nil })
            let coords = dragTargets.compactMap { parseCoordinate($0) }
            if coords.count == dragTargets.count && coords.count >= 2 {
                return try await provider.drag(
                    path: Array(coords), options: dragOptions, app: appName)
            } else if dragTargets.count == 2,
                let fromRef = ElementRef.parse(String(dragTargets.first!)),
                let toRef = ElementRef.parse(String(dragTargets.last!))
            {
                guard let appName else {
                    throw ForepawError.actionFailed("drag requires --app (on action or batch)")
                }
                return try await provider.drag(
                    fromRef: fromRef, toRef: toRef, app: appName, options: dragOptions)
            } else {
                throw ForepawError.actionFailed(
                    "Invalid drag targets. Use coordinates (500,300) or refs (@e3)")
            }

        case "wait":
            let text = parseOption("--text", from: actionArgs) ?? actionArgs.first
            guard let text else {
                throw ForepawError.actionFailed("wait requires text to search for")
            }
            let appName = parseOption("--app", from: actionArgs) ?? app
            guard let appName else {
                throw ForepawError.actionFailed("wait requires --app (on action or batch)")
            }
            let win = parseOption("--window", from: actionArgs) ?? window
            let timeout = parseOption("--timeout", from: actionArgs).flatMap { Double($0) } ?? 10
            let interval = parseOption("--interval", from: actionArgs).flatMap { Double($0) } ?? 1
            return try await provider.wait(
                text: text, app: appName, window: win,
                timeout: timeout, interval: interval)

        default:
            throw ForepawError.actionFailed(
                "Unknown action '\(command)'. Supported: click, hover, drag, type, keyboard-type, press, scroll, ocr-click, wait"
            )
        }
    }

    /// Parse a named option value from an argument list.
    private func parseOption(_ name: String, from args: [String]) -> String? {
        guard let idx = args.firstIndex(of: name), idx + 1 < args.count else { return nil }
        return args[idx + 1]
    }

    /// Collect remaining positional arguments as text, skipping flags and their values.
    /// `skip` is the number of leading positional args to skip (e.g. 1 for `type @ref text...`).
    /// Returns nil if no positional text remains.
    private func collectPositionalText(from args: [String], skip: Int) -> String? {
        var positional: [String] = []
        var i = 0
        while i < args.count {
            if args[i].starts(with: "--") {
                i += 2  // skip flag + its value
                continue
            }
            positional.append(args[i])
            i += 1
        }
        let remaining = positional.dropFirst(skip)
        guard !remaining.isEmpty else { return nil }
        return remaining.joined(separator: " ")
    }

    /// Split a string into shell-like tokens, respecting double quotes.
    private func shellSplit(_ input: String) -> [String] {
        var tokens: [String] = []
        var current = ""
        var inQuotes = false

        for char in input {
            if char == "\"" {
                inQuotes.toggle()
            } else if char == " " && !inQuotes {
                if !current.isEmpty {
                    tokens.append(current)
                    current = ""
                }
            } else {
                current.append(char)
            }
        }
        if !current.isEmpty {
            tokens.append(current)
        }
        return tokens
    }
}

struct Drag: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Drag from one point to another (for drawing, moving, resizing)"
    )

    @Argument(
        parsing: .remaining,
        help: "Drag targets: <from> <to> or path <p1> <p2> <p3>... (coords as x,y or refs as @eN)")
    var targets: [String] = []

    @Option(name: .long, help: "Target application name")
    var app: String?

    @Option(name: .long, help: "Number of intermediate steps per segment (default 30, higher = smoother)")
    var steps: Int = 30

    @Option(name: .long, help: "Total duration in seconds (default 0.3)")
    var duration: Double = 0.3

    @Option(name: .long, help: "Hold modifier keys during drag (e.g. shift, shift+alt, cmd+shift)")
    var modifiers: String?

    @Option(name: .long, help: "Mouse pressure 0.0-1.0 (for drawing tablet simulation)")
    var pressure: Double?

    @Flag(name: .long, help: "Use right mouse button (for context drag, canvas panning)")
    var right: Bool = false

    @Flag(name: .long, help: "Close path by appending start point (for shapes, 3+ points)")
    var close: Bool = false

    @Flag(name: .long, help: "Read coordinates from stdin (one x,y per line, or space-separated)")
    var stdin: Bool = false

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let options = buildDragOptions()
        let result: ActionResult

        if stdin {
            let coords = try readCoordsFromStdin()
            guard coords.count >= 2 else {
                throw ValidationError(
                    "--stdin requires at least 2 coordinates. Got \(coords.count).\n"
                        + "Pipe coordinates as: echo \"100,100 200,200 300,300\" | forepaw drag --stdin --app App"
                )
            }
            result = try await provider.drag(path: coords, options: options, app: app)
        } else {
            guard targets.count >= 2 else {
                throw ValidationError(
                    "drag requires at least 2 targets: <from> <to> or path of coordinates.\n"
                        + "Examples: drag 100,100 500,500 --app Finder\n"
                        + "          drag @e3 @e7 --app Finder\n"
                        + "          drag 100,100 300,200 500,100 --app App  (path through 3 points)")
            }

            // Check if all targets are coordinates
            let coords = targets.compactMap { parseCoordinate($0) }
            if coords.count == targets.count {
                result = try await provider.drag(path: coords, options: options, app: app)
            } else if targets.count == 2,
                let fromRef = ElementRef.parse(targets[0]),
                let toRef = ElementRef.parse(targets[1])
            {
                guard let app else {
                    throw ValidationError("--app is required for ref-based drag")
                }
                result = try await provider.drag(
                    fromRef: fromRef, toRef: toRef, app: app, options: options)
            } else if targets.count == 2 {
                // Mixed: one might be a ref, the other coords. Resolve each.
                let from = try resolveDragTarget(targets[0], provider: provider, app: app)
                let to = try resolveDragTarget(targets[1], provider: provider, app: app)
                result = try await provider.drag(path: [from, to], options: options, app: app)
            } else {
                throw ValidationError(
                    "Path mode requires all coordinates (e.g. drag 100,100 300,200 500,100). Refs only supported for 2-point drag."
                )
            }
        }

        let formatter = OutputFormatter(json: json)
        print(
            formatter.format(
                success: result.success, command: "drag", data: ["text": result.message ?? "dragged"]))
    }

    /// Read coordinates from stdin. Accepts space-separated or newline-separated x,y pairs.
    private func readCoordsFromStdin() throws -> [Point] {
        let data = FileHandle.standardInput.availableData
        guard let input = String(data: data, encoding: .utf8)
        else {
            throw ValidationError("Failed to read from stdin")
        }

        // Split on whitespace and newlines, parse each token as x,y
        let tokens = input.split(whereSeparator: { $0.isWhitespace || $0.isNewline })
        let coords = tokens.compactMap { parseCoordinate(String($0)) }

        if coords.count != tokens.count {
            let bad = tokens.filter { parseCoordinate(String($0)) == nil }
            throw ValidationError(
                "Invalid coordinate(s): \(bad.joined(separator: ", ")). Expected x,y format.")
        }
        return coords
    }

    private func buildDragOptions() -> DragOptions {
        return DragOptions(
            steps: steps, duration: duration,
            modifiers: KeyCombo.Modifier.parseModifiers(modifiers),
            pressure: pressure,
            rightButton: right, closePath: close)
    }

    private func resolveDragTarget(_ target: String, provider: any DesktopProvider, app: String?) throws -> Point {
        if let point = parseCoordinate(target) {
            return point
        }
        if let ref = ElementRef.parse(target) {
            guard let app else {
                throw ValidationError("--app is required for ref-based drag")
            }
            return try provider.resolveRefPosition(ref, app: app)
        }
        throw ValidationError("Invalid target: \(target). Expected a ref (@e1) or coordinates (500,300).")
    }
}

/// Resolve text from either positional argument or --text option.
/// Errors if neither is provided or both are provided.
func resolveText(positional: String?, option: String?, command: String) throws -> String {
    if option != nil, positional != nil {
        throw ValidationError(
            "Provide text as either a positional argument or --text, not both.")
    }
    guard let text = option ?? positional else {
        throw ValidationError(
            "\(command) requires text. Provide as argument or use --text for text starting with dashes.")
    }
    return text
}

/// Parse "x,y" coordinate string into a Point.
func parseCoordinate(_ string: String) -> Point? {
    let parts = string.split(separator: ",")
    guard parts.count == 2,
        let x = Double(parts[0].trimmingCharacters(in: .whitespaces)),
        let y = Double(parts[1].trimmingCharacters(in: .whitespaces))
    else { return nil }
    return Point(x: x, y: y)
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

    @Option(name: .long, help: "Window-relative coordinates to scroll at (e.g. 200,400)")
    var at: String?

    @Flag(name: .long, help: "JSON output")
    var json: Bool = false

    mutating func run() async throws {
        let validDirections = ["up", "down", "left", "right"]
        guard validDirections.contains(direction) else {
            throw ValidationError("Invalid direction '\(direction)'. Use: \(validDirections.joined(separator: ", "))")
        }

        if ref != nil, at != nil {
            throw ValidationError("Use --ref or --at, not both")
        }

        var elementRef: ElementRef?
        if let refStr = ref {
            guard let parsed = ElementRef.parse(refStr) else {
                throw ValidationError("Invalid ref: \(refStr). Expected format: @e1, @e2, etc.")
            }
            elementRef = parsed
        }

        var scrollPoint: Point?
        if let at = at {
            guard let point = parseCoordinate(at) else {
                throw ValidationError("Invalid coordinates: \(at). Expected x,y (e.g. 589,400)")
            }
            scrollPoint = point
        }

        let result = try await provider.scroll(
            direction: direction, amount: amount, app: app, window: window,
            ref: elementRef, at: scrollPoint)
        let formatter = OutputFormatter(json: json)
        print(
            formatter.format(success: result.success, command: "scroll", data: ["text": result.message ?? "scrolled"]))
    }
}
