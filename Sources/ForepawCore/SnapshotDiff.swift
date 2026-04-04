import Foundation

// MARK: - Diff result

/// Result of comparing two rendered snapshot texts.
public struct SnapshotDiff: Sendable {
    public let lines: [DiffLine]

    /// Lines that were added (present only in the new snapshot).
    public var added: [DiffLine] { lines.filter { $0.kind == .added } }
    /// Lines that were removed (present only in the old snapshot).
    public var removed: [DiffLine] { lines.filter { $0.kind == .removed } }
    /// Lines present in both (may have different refs).
    public var unchanged: [DiffLine] { lines.filter { $0.kind == .unchanged } }

    public var isEmpty: Bool { added.isEmpty && removed.isEmpty }

    public var summary: String {
        let a = added.count
        let r = removed.count
        let u = unchanged.count
        if a == 0 && r == 0 { return "no changes" }
        var parts: [String] = []
        if a > 0 { parts.append("\(a) added") }
        if r > 0 { parts.append("\(r) removed") }
        parts.append("\(u) unchanged")
        return parts.joined(separator: ", ")
    }

    public init(lines: [DiffLine]) {
        self.lines = lines
    }
}

public struct DiffLine: Sendable {
    public enum Kind: Sendable {
        case added
        case removed
        case unchanged
    }

    public let kind: Kind
    /// The full line text (with refs, indentation, etc.)
    public let text: String

    public init(kind: Kind, text: String) {
        self.kind = kind
        self.text = text
    }
}

// MARK: - Diff renderer

extension SnapshotDiff {
    /// Render the diff as text with +/- markers, similar to unified diff.
    /// Only shows added and removed lines (unchanged lines are omitted for brevity).
    /// Context lines around changes can be included with `context` parameter.
    public func render(context: Int = 0) -> String {
        var output: [String] = []

        if isEmpty {
            output.append("[no changes]")
            return output.joined(separator: "\n")
        }

        output.append("[diff: \(summary)]")
        output.append("")

        if context == 0 {
            // Simple mode: just show added/removed
            for line in lines {
                switch line.kind {
                case .added:
                    output.append("+ \(line.text)")
                case .removed:
                    output.append("- \(line.text)")
                case .unchanged:
                    break
                }
            }
        } else {
            // Context mode: show unchanged lines near changes
            let changeIndices = Set(
                lines.enumerated()
                    .filter { $0.element.kind != .unchanged }
                    .map(\.offset)
            )

            var visibleIndices = Set<Int>()
            for idx in changeIndices {
                for c in max(0, idx - context)...min(lines.count - 1, idx + context) {
                    visibleIndices.insert(c)
                }
            }

            var lastPrinted = -2  // track gaps for "..." separator
            for (i, line) in lines.enumerated() {
                guard visibleIndices.contains(i) else { continue }
                if i > lastPrinted + 1 && lastPrinted >= 0 {
                    output.append("  ...")
                }
                switch line.kind {
                case .added:
                    output.append("+ \(line.text)")
                case .removed:
                    output.append("- \(line.text)")
                case .unchanged:
                    output.append("  \(line.text)")
                }
                lastPrinted = i
            }
        }

        return output.joined(separator: "\n")
    }
}

// MARK: - Differ

/// Compares two rendered snapshot texts, producing a line-level diff.
///
/// Refs (`@eN`) are stripped for comparison purposes so that positional
/// ref shifts (caused by elements being added/removed earlier in the tree)
/// don't produce false "changed" lines. The output includes the full
/// original lines with their refs intact.
public struct SnapshotDiffer: Sendable {
    public init() {}

    /// Compare two rendered snapshot texts.
    /// The first line of each text (the "app:" header) is skipped.
    public func diff(old: String, new: String) -> SnapshotDiff {
        let oldLines = old.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        let newLines = new.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)

        // Skip the "app:" header line if present
        let oldContent = oldLines.first?.hasPrefix("app:") == true ? Array(oldLines.dropFirst()) : oldLines
        let newContent = newLines.first?.hasPrefix("app:") == true ? Array(newLines.dropFirst()) : newLines

        // Strip refs for comparison
        let oldStripped = oldContent.map { Self.stripRefs($0) }
        let newStripped = newContent.map { Self.stripRefs($0) }

        // Compute LCS-based diff on stripped lines
        let diffOps = lcs(old: oldStripped, new: newStripped)

        // Map back to original lines with refs
        var result: [DiffLine] = []
        for op in diffOps {
            switch op {
            case .keep(_, let newIdx):
                result.append(DiffLine(kind: .unchanged, text: newContent[newIdx]))
            case .insert(let newIdx):
                result.append(DiffLine(kind: .added, text: newContent[newIdx]))
            case .delete(let oldIdx):
                result.append(DiffLine(kind: .removed, text: oldContent[oldIdx]))
            }
        }

        return SnapshotDiff(lines: result)
    }

    // MARK: - Ref stripping

    /// Remove @eN refs from a line for comparison purposes.
    /// "  button @e5 "OK" (100,200 80x30)" -> "  button "OK" (100,200 80x30)"
    static func stripRefs(_ line: String) -> String {
        // Match @e followed by digits, plus any trailing space
        var result = line
        while let range = result.range(of: #"@e\d+\s?"#, options: .regularExpression) {
            result.removeSubrange(range)
        }
        // Trim any trailing whitespace left over
        while result.hasSuffix(" ") {
            result.removeLast()
        }
        return result
    }

    // MARK: - LCS diff algorithm

    enum DiffOp {
        case keep(oldIdx: Int, newIdx: Int)
        case insert(newIdx: Int)
        case delete(oldIdx: Int)
    }

    /// Simple LCS-based diff. O(nm) space and time -- fine for snapshots (<1000 lines).
    func lcs(old: [String], new: [String]) -> [DiffOp] {
        let m = old.count
        let n = new.count

        // Build LCS table
        var table = Array(repeating: Array(repeating: 0, count: n + 1), count: m + 1)
        for i in 1...max(m, 1) {
            guard i <= m else { break }
            for j in 1...max(n, 1) {
                guard j <= n else { break }
                if old[i - 1] == new[j - 1] {
                    table[i][j] = table[i - 1][j - 1] + 1
                } else {
                    table[i][j] = max(table[i - 1][j], table[i][j - 1])
                }
            }
        }

        // Backtrack to produce diff operations
        var ops: [DiffOp] = []
        var i = m
        var j = n
        while i > 0 || j > 0 {
            if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
                ops.append(.keep(oldIdx: i - 1, newIdx: j - 1))
                i -= 1
                j -= 1
            } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
                ops.append(.insert(newIdx: j - 1))
                j -= 1
            } else {
                ops.append(.delete(oldIdx: i - 1))
                i -= 1
            }
        }

        return ops.reversed()
    }
}

// MARK: - Snapshot cache

/// Caches rendered snapshot text to temp files for cross-invocation diffing.
public struct SnapshotCache: Sendable {
    public init() {}

    /// Save rendered snapshot text for an app.
    public func save(app: String, text: String) throws {
        let path = cachePath(for: app)
        try text.write(toFile: path, atomically: true, encoding: .utf8)
    }

    /// Load the last cached snapshot text for an app, if any.
    public func load(app: String) -> String? {
        let path = cachePath(for: app)
        return try? String(contentsOfFile: path, encoding: .utf8)
    }

    /// Remove cached snapshot for an app.
    public func clear(app: String) {
        let path = cachePath(for: app)
        try? FileManager.default.removeItem(atPath: path)
    }

    private func cachePath(for app: String) -> String {
        let sanitized = app.lowercased().replacingOccurrences(of: " ", with: "-")
        return NSTemporaryDirectory() + "forepaw-snapshot-\(sanitized).txt"
    }
}
