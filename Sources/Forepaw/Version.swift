import Foundation

/// Base version. Updated at release time.
let baseVersion = "0.3.0"

/// Full version string. In dev builds, appends the git short hash.
/// Set by build tooling via FOREPAW_VERSION env var, or auto-detected.
let forepawVersion: String = {
    // CI/mise can set this explicitly
    if let env = ProcessInfo.processInfo.environment["FOREPAW_VERSION"], !env.isEmpty {
        return env
    }

    // Try to detect git hash for dev builds
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
    process.arguments = ["describe", "--always", "--dirty"]
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = FileHandle.nullDevice

    do {
        try process.run()
        process.waitUntilExit()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        if let hash = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
            !hash.isEmpty
        {
            return "\(baseVersion)-dev+\(hash)"
        }
    } catch {}

    return "\(baseVersion)-dev"
}()
