import ApplicationServices
import Cocoa
import ForepawCore

/// A resolved CGWindowList window with its ID, title, and bounds.
public struct ResolvedWindow: Sendable {
    public let windowID: CGWindowID
    public let title: String
    public let bounds: [String: Double]

    public var origin: CGPoint {
        CGPoint(x: bounds["X"] ?? 0, y: bounds["Y"] ?? 0)
    }

    public var size: CGSize {
        CGSize(width: bounds["Width"] ?? 0, height: bounds["Height"] ?? 0)
    }

    public var center: CGPoint {
        CGPoint(x: origin.x + size.width / 2, y: origin.y + size.height / 2)
    }
}

/// macOS implementation of `DesktopProvider` using Accessibility APIs.
public final class DarwinProvider: DesktopProvider, @unchecked Sendable {
    // Current snapshot's ref table, keyed by ref ID.
    // Stores AXUIElement handles for action dispatch.
    internal var refTable: [ElementRef: AXUIElement] = [:]

    public init() {}

    // MARK: - Permissions

    public func hasPermissions() -> Bool {
        AXIsProcessTrusted()
    }

    public func hasScreenRecordingPermission() -> Bool {
        CGPreflightScreenCaptureAccess()
    }

    public func requestPermissions() -> Bool {
        let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
        return AXIsProcessTrustedWithOptions(options)
    }

    public func requestScreenRecordingPermission() -> Bool {
        CGRequestScreenCaptureAccess()
    }

    // MARK: - DesktopProvider

    public func listApps() async throws -> [AppInfo] {
        NSWorkspace.shared.runningApplications
            .filter { $0.activationPolicy == .regular }
            .compactMap { app in
                guard let name = app.localizedName else { return nil }
                return AppInfo(
                    name: name,
                    bundleID: app.bundleIdentifier,
                    pid: app.processIdentifier
                )
            }
    }

    public func findWindow(pid: Int32, window: String? = nil) throws -> ResolvedWindow {
        let windowList = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []

        // Collect all real windows for this app (skip phantoms)
        var appWindows: [(id: CGWindowID, title: String, bounds: [String: Double])] = []
        for info in windowList {
            guard let ownerPID = info[kCGWindowOwnerPID as String] as? Int32,
                ownerPID == pid,
                let bounds = info[kCGWindowBounds as String] as? [String: Double]
            else { continue }
            let w = bounds["Width"] ?? 0
            let h = bounds["Height"] ?? 0
            guard w >= 10 && h >= 10 else { continue }
            let windowID = (info[kCGWindowNumber as String] as? Int).map { CGWindowID($0) } ?? 0
            let title = info[kCGWindowName as String] as? String ?? ""
            appWindows.append((id: windowID, title: title, bounds: bounds))
        }

        guard !appWindows.isEmpty else {
            throw ForepawError.windowNotFound(window ?? "any")
        }

        if let window = window {
            // Match by window ID: "w-1234"
            if window.hasPrefix("w-"), let idNum = UInt32(window.dropFirst(2)) {
                if let match = appWindows.first(where: { $0.id == CGWindowID(idNum) }) {
                    return ResolvedWindow(windowID: match.id, title: match.title, bounds: match.bounds)
                }
                throw ForepawError.windowNotFound(window)
            }

            // Substring match on title (case-insensitive)
            let matches = appWindows.filter {
                $0.title.localizedCaseInsensitiveContains(window)
            }
            if matches.count == 1 {
                let m = matches[0]
                return ResolvedWindow(windowID: m.id, title: m.title, bounds: m.bounds)
            }
            if matches.count > 1 {
                let titles = matches.map { "  w-\($0.id)  \($0.title)" }.joined(separator: "\n")
                throw ForepawError.ambiguousWindow(window, titles)
            }
            throw ForepawError.windowNotFound(window)
        }

        // Default: largest window by area
        let best = appWindows.max(by: {
            let a1 = ($0.bounds["Width"] ?? 0) * ($0.bounds["Height"] ?? 0)
            let a2 = ($1.bounds["Width"] ?? 0) * ($1.bounds["Height"] ?? 0)
            return a1 < a2
        })!
        return ResolvedWindow(windowID: best.id, title: best.title, bounds: best.bounds)
    }

    public func listWindows(app appName: String?) async throws -> [WindowInfo] {
        let windowList = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []

        return windowList.compactMap { info -> WindowInfo? in
            guard let name = info[kCGWindowOwnerName as String] as? String,
                let windowID = info[kCGWindowNumber as String] as? Int,
                let title = info[kCGWindowName as String] as? String
            else { return nil }

            if let filter = appName, name != filter { return nil }

            // Skip phantom/tiny windows
            if let boundsDict = info[kCGWindowBounds as String] as? [String: Double] {
                let w = boundsDict["Width"] ?? 0
                let h = boundsDict["Height"] ?? 0
                if w < 10 || h < 10 { return nil }
            }

            var bounds: Rect?
            if let boundsDict = info[kCGWindowBounds as String] as? [String: Double] {
                bounds = Rect(
                    x: boundsDict["X"] ?? 0,
                    y: boundsDict["Y"] ?? 0,
                    width: boundsDict["Width"] ?? 0,
                    height: boundsDict["Height"] ?? 0
                )
            }

            return WindowInfo(id: "w-\(windowID)", title: title, app: name, bounds: bounds)
        }
    }

    // MARK: - Private helpers

    internal func findApp(named name: String) throws -> NSRunningApplication {
        let apps = NSWorkspace.shared.runningApplications.filter { $0.activationPolicy == .regular }
        if let app = apps.first(where: { $0.localizedName == name }) {
            return app
        }
        if let app = apps.first(where: { $0.bundleIdentifier == name }) {
            return app
        }
        // Case-insensitive partial match
        if let app = apps.first(where: { $0.localizedName?.localizedCaseInsensitiveContains(name) == true }) {
            return app
        }
        throw ForepawError.appNotFound(name)
    }

}
