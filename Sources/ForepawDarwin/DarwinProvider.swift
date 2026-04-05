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
    /// Default depth for AX tree walks, from SnapshotOptions.defaultDepth.
    /// Used by resolveRef to match snapshot's ref assignment.
    static let defaultDepth = SnapshotOptions.defaultDepth

    /// Depth for Electron apps, which nest web content deeply (13+ levels of DOM groups).
    static let electronDepth = 25

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

        // Multi-process fallback: some apps (Steam, etc.) render their UI in a helper
        // process with `accessory` activation policy. The main process (which appears in
        // list-apps) has no usable windows. When that happens, look for onscreen windows
        // from processes sharing the same bundle ID prefix.
        if appWindows.isEmpty {
            if let mainApp = NSWorkspace.shared.runningApplications.first(where: {
                $0.processIdentifier == pid
            }), let mainBundle = mainApp.bundleIdentifier {
                let helperPIDs = NSWorkspace.shared.runningApplications
                    .filter { helper in
                        helper.processIdentifier != pid
                            && (helper.bundleIdentifier ?? "").hasPrefix(mainBundle)
                    }
                    .map { $0.processIdentifier }
                let helperPIDSet = Set(helperPIDs)

                for info in windowList {
                    guard let ownerPID = info[kCGWindowOwnerPID as String] as? Int32,
                        helperPIDSet.contains(ownerPID),
                        let bounds = info[kCGWindowBounds as String] as? [String: Double]
                    else { continue }
                    let w = bounds["Width"] ?? 0
                    let h = bounds["Height"] ?? 0
                    guard w >= 10 && h >= 10 else { continue }
                    let windowID =
                        (info[kCGWindowNumber as String] as? Int).map { CGWindowID($0) } ?? 0
                    let title = info[kCGWindowName as String] as? String ?? ""
                    appWindows.append((id: windowID, title: title, bounds: bounds))
                }
            }
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

        // Default: prefer titled windows, then largest by area.
        // Finder (and some other apps) have a full-screen untitled desktop window
        // that's larger than the actual content window.
        let titled = appWindows.filter { !$0.title.isEmpty }
        let candidates = titled.isEmpty ? appWindows : titled
        let best = candidates.max(by: {
            let a1 = ($0.bounds["Width"] ?? 0) * ($0.bounds["Height"] ?? 0)
            let a2 = ($1.bounds["Width"] ?? 0) * ($1.bounds["Height"] ?? 0)
            return a1 < a2
        })!
        return ResolvedWindow(windowID: best.id, title: best.title, bounds: best.bounds)
    }

    public func listWindows(app appName: String?) async throws -> [WindowInfo] {
        let windowList = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []

        // Build set of PIDs that belong to this app (including helper processes)
        var allowedPIDs: Set<Int32>?
        if let filter = appName {
            let mainApp = try? findApp(named: filter)
            if let app = mainApp, let bundle = app.bundleIdentifier {
                var pids: Set<Int32> = [app.processIdentifier]
                for helper in NSWorkspace.shared.runningApplications
                where (helper.bundleIdentifier ?? "").hasPrefix(bundle)
                    && helper.processIdentifier != app.processIdentifier
                {
                    pids.insert(helper.processIdentifier)
                }
                allowedPIDs = pids
            } else if let app = mainApp {
                allowedPIDs = [app.processIdentifier]
            }
        }

        return windowList.compactMap { info -> WindowInfo? in
            guard let name = info[kCGWindowOwnerName as String] as? String,
                let windowID = info[kCGWindowNumber as String] as? Int,
                let title = info[kCGWindowName as String] as? String
            else { return nil }

            if let pids = allowedPIDs {
                guard let ownerPID = info[kCGWindowOwnerPID as String] as? Int32,
                    pids.contains(ownerPID)
                else { return nil }
            } else if let filter = appName, name != filter {
                return nil
            }

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

    // MARK: - Electron detection & accessibility

    /// Check if an app bundle contains the Electron Framework.
    /// CEF (Chromium Embedded Framework) apps like Spotify are NOT included --
    /// CEF doesn't respond to `AXManualAccessibility` and exposes only empty
    /// group nodes without a web content tree. CEF apps need OCR.
    internal func isElectronApp(_ app: NSRunningApplication) -> Bool {
        guard let bundleURL = app.bundleURL else { return false }
        let frameworkPath = bundleURL.appendingPathComponent(
            "Contents/Frameworks/Electron Framework.framework")
        return FileManager.default.fileExists(atPath: frameworkPath.path)
    }

    /// Tell an Electron app to build its Chromium accessibility tree.
    /// This sets the `AXManualAccessibility` attribute, which is Electron's
    /// official API for third-party assistive technology on macOS.
    /// The call is idempotent -- setting it when already enabled is a no-op.
    internal func enableElectronAccessibility(_ app: NSRunningApplication) {
        let appElement = AXUIElementCreateApplication(app.processIdentifier)
        AXUIElementSetAttributeValue(
            appElement, "AXManualAccessibility" as CFString, true as CFTypeRef)
    }

    /// Check if an Electron app's web content tree is populated yet.
    /// After setting AXManualAccessibility, Chromium needs time to build the tree.
    /// We detect this by looking for an AXWebArea with actual children.
    internal func electronTreeIsPopulated(_ app: NSRunningApplication) -> Bool {
        let appElement = AXUIElementCreateApplication(app.processIdentifier)
        return hasPopulatedWebArea(appElement, depth: 0, maxDepth: 10)
    }

    private func hasPopulatedWebArea(_ element: AXUIElement, depth: Int, maxDepth: Int) -> Bool {
        guard depth < maxDepth else { return false }
        let role = getAttribute(element, kAXRoleAttribute) as? String
        if role == "AXWebArea" {
            // Check if it has interactive children (not just empty groups)
            if let children = getAttribute(element, kAXChildrenAttribute) as? [AXUIElement] {
                for child in children {
                    let childRole = getAttribute(child, kAXRoleAttribute) as? String ?? ""
                    if ElementNode.isInteractiveRole(childRole) { return true }
                    // Check one level deeper for interactive content inside groups
                    if let grandchildren = getAttribute(child, kAXChildrenAttribute) as? [AXUIElement] {
                        for gc in grandchildren {
                            let gcRole = getAttribute(gc, kAXRoleAttribute) as? String ?? ""
                            if ElementNode.isInteractiveRole(gcRole) { return true }
                        }
                    }
                }
            }
            return false
        }
        if let children = getAttribute(element, kAXChildrenAttribute) as? [AXUIElement] {
            for child in children {
                if hasPopulatedWebArea(child, depth: depth + 1, maxDepth: maxDepth) { return true }
            }
        }
        return false
    }

}
