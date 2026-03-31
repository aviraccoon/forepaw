import ArgumentParser
import ForepawCore
import ForepawDarwin
import Foundation

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
