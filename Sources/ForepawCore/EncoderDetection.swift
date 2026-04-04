import Foundation

/// Detects available image encoders at runtime.
/// Results are cached for the process lifetime (~0.7ms to check).
public enum EncoderDetection: Sendable {
    /// Best image format available on this system.
    /// WebP (via cwebp) if available, otherwise JPEG.
    public static let bestFormat: ImageFormat = {
        return isAvailable("cwebp") ? .webp : .jpeg
    }()

    /// Check whether a command-line tool is available in PATH.
    public static func isAvailable(_ command: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["which", command]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }
}
