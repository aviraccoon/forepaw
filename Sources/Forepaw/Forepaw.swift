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
            Drag.self,
            Hover.self,
            Wait.self,
            Batch.self,
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
