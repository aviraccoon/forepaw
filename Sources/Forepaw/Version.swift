import Foundation

/// Base version. Updated at release time.
let baseVersion = "0.3.0"

/// Full version string. Dev builds append the git short hash.
///
/// Release builds set FOREPAW_BUILD_VERSION at compile time via
/// `-Xswiftc -DRELEASE_BUILD` -- when that flag is present, this
/// returns the base version with no suffix.
///
/// Dev builds shell out to `git rev-parse --short HEAD` to get the
/// current commit hash. Only runs inside a git repo (the binary's
/// own directory must contain .git). Outside a git repo (installed
/// binary), returns just the base version.
let forepawVersion: String = {
    #if RELEASE_BUILD
        return baseVersion
    #else
        // Dev build: try to find git hash, but only if we're in the source repo
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = ["rev-parse", "--short", "HEAD"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice

        do {
            try process.run()
            process.waitUntilExit()
            guard process.terminationStatus == 0 else { return baseVersion }
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            if let hash = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
                !hash.isEmpty
            {
                return "\(baseVersion)-dev+\(hash)"
            }
        } catch {}

        return baseVersion
    #endif
}()
