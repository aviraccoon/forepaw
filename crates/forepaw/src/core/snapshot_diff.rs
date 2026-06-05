//! Snapshot diffing via LCS algorithm.
/// Snapshot diffing via LCS algorithm.
///
/// Refs (@eN) are stripped for comparison so positional ref shifts
/// don't produce false "changed" lines.
///
/// A single line in a diff result.
#[derive(Debug, Clone)]
pub struct DiffLine {
    /// Whether the line was added, removed, or unchanged.
    pub kind: DiffLineKind,
    /// The line content (with refs preserved).
    pub text: String,
}

/// Classification of a diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiffLineKind {
    /// Line present only in the new snapshot.
    Added,
    /// Line present only in the old snapshot.
    Removed,
    /// Line present in both (unchanged).
    Unchanged,
}

/// Result of comparing two rendered snapshot texts.
#[derive(Debug, Clone)]
pub struct SnapshotDiff {
    /// The diffed lines.
    pub lines: Vec<DiffLine>,
}

impl SnapshotDiff {
    /// Create a new diff from the given lines.
    #[must_use]
    pub fn new(lines: Vec<DiffLine>) -> Self {
        Self { lines }
    }

    /// Lines that were added (present only in the new snapshot).
    #[must_use]
    pub fn added(&self) -> Vec<&DiffLine> {
        self.lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Added)
            .collect()
    }

    /// Lines that were removed (present only in the old snapshot).
    #[must_use]
    pub fn removed(&self) -> Vec<&DiffLine> {
        self.lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Removed)
            .collect()
    }

    /// Lines present in both.
    #[must_use]
    pub fn unchanged(&self) -> Vec<&DiffLine> {
        self.lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Unchanged)
            .collect()
    }

    /// Whether there are no changes (no added or removed lines).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added().is_empty() && self.removed().is_empty()
    }

    /// A human-readable summary string (e.g. "2 added, 1 removed, 5 unchanged").
    #[must_use]
    pub fn summary(&self) -> String {
        let a = self.added().len();
        let r = self.removed().len();
        let u = self.unchanged().len();
        if a == 0 && r == 0 {
            return "no changes".to_owned();
        }
        let mut parts: Vec<String> = Vec::new();
        if a > 0 {
            parts.push(format!("{a} added"));
        }
        if r > 0 {
            parts.push(format!("{r} removed"));
        }
        parts.push(format!("{u} unchanged"));
        parts.join(", ")
    }

    /// Render the diff as text with +/- markers.
    /// Context lines around changes can be included with `context` parameter.
    #[must_use]
    pub fn render(&self, context: usize) -> String {
        let mut output: Vec<String> = Vec::new();

        if self.is_empty() {
            output.push("[no changes]".to_owned());
            return output.join("\n");
        }

        output.push(format!("[diff: {}]", self.summary()));
        output.push(String::new());

        if context == 0 {
            // Simple mode: just show added/removed
            for line in &self.lines {
                match line.kind {
                    DiffLineKind::Added => output.push(format!("+ {}", line.text)),
                    DiffLineKind::Removed => output.push(format!("- {}", line.text)),
                    DiffLineKind::Unchanged => {}
                }
            }
        } else {
            // Context mode: show unchanged lines near changes
            let change_indices: Vec<usize> = self
                .lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.kind != DiffLineKind::Unchanged)
                .map(|(i, _)| i)
                .collect();

            let mut visible_indices: std::collections::HashSet<usize> =
                std::collections::HashSet::new();
            for &idx in &change_indices {
                let start = idx.saturating_sub(context);
                let end = (idx + context).min(self.lines.len() - 1);
                for c in start..=end {
                    visible_indices.insert(c);
                }
            }

            let mut last_printed: Option<usize> = None;
            for (i, line) in self.lines.iter().enumerate() {
                if !visible_indices.contains(&i) {
                    continue;
                }
                if last_printed.is_some_and(|last| i > last + 1) {
                    output.push("  ...".to_owned());
                }
                match line.kind {
                    DiffLineKind::Added => output.push(format!("+ {}", line.text)),
                    DiffLineKind::Removed => output.push(format!("- {}", line.text)),
                    DiffLineKind::Unchanged => output.push(format!("  {}", line.text)),
                }
                last_printed = Some(i);
            }
        }

        output.join("\n")
    }
}

/// Removes `@eN` refs from a line for comparison purposes.
///
/// Strips every occurrence of `@e` followed by one or more digits,
/// consuming an optional trailing space after each ref.
/// Trailing whitespace is trimmed from the result.
#[must_use]
pub fn strip_refs(line: &str) -> String {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len);
    let mut last = 0;
    let mut i = 0;

    while i < len {
        #[expect(
            clippy::indexing_slicing,
            reason = "all indices guarded by < len checks"
        )]
        if bytes[i] == b'@' && i + 1 < len && bytes[i + 1] == b'e' {
            let digit_start = i + 2;
            if digit_start < len && bytes[digit_start].is_ascii_digit() {
                // Copy everything before this ref
                #[expect(clippy::string_slice, reason = "i is always at an ASCII byte boundary")]
                result.push_str(&line[last..i]);
                // Consume digits
                let mut end = digit_start + 1;
                while end < len && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                // Consume optional trailing space
                if end < len && bytes[end] == b' ' {
                    end += 1;
                }
                last = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }

    #[expect(
        clippy::string_slice,
        reason = "last is always at an ASCII byte boundary"
    )]
    result.push_str(&line[last..]);
    result.trim_end().to_owned()
}

/// Compares two rendered snapshot texts, producing a line-level diff.
#[derive(Debug)]
pub struct SnapshotDiffer;

impl SnapshotDiffer {
    /// Create a new differ.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Compare two rendered snapshot texts.
    /// The first line of each text (the "app:" header) is skipped.
    #[expect(
        clippy::indexing_slicing,
        reason = "LCS algorithm uses validated indices from diff ops"
    )]
    #[must_use]
    pub fn diff(&self, old: &str, new: &str) -> SnapshotDiff {
        let old_lines: Vec<&str> = old.split('\n').collect();
        let new_lines: Vec<&str> = new.split('\n').collect();

        // Skip the "app:" header line if present
        let old_content: Vec<&str> = if old_lines.first().is_some_and(|l| l.starts_with("app:")) {
            old_lines[1..].to_vec()
        } else {
            old_lines.clone()
        };
        let new_content: Vec<&str> = if new_lines.first().is_some_and(|l| l.starts_with("app:")) {
            new_lines[1..].to_vec()
        } else {
            new_lines.clone()
        };

        // Strip refs for comparison
        let old_stripped: Vec<String> = old_content.iter().map(|l| strip_refs(l)).collect();
        let new_stripped: Vec<String> = new_content.iter().map(|l| strip_refs(l)).collect();

        // Compute LCS-based diff on stripped lines
        let diff_ops = lcs(&old_stripped, &new_stripped);

        // Map back to original lines with refs
        let mut result: Vec<DiffLine> = Vec::new();
        for op in diff_ops {
            match op {
                DiffOp::Keep { new_idx } => {
                    result.push(DiffLine {
                        kind: DiffLineKind::Unchanged,
                        text: new_content[new_idx].to_owned(),
                    });
                }
                DiffOp::Insert { new_idx } => {
                    result.push(DiffLine {
                        kind: DiffLineKind::Added,
                        text: new_content[new_idx].to_owned(),
                    });
                }
                DiffOp::Delete { old_idx } => {
                    result.push(DiffLine {
                        kind: DiffLineKind::Removed,
                        text: old_content[old_idx].to_owned(),
                    });
                }
            }
        }

        SnapshotDiff::new(result)
    }
}

impl Default for SnapshotDiffer {
    fn default() -> Self {
        Self::new()
    }
}

enum DiffOp {
    Keep { new_idx: usize },
    Insert { new_idx: usize },
    Delete { old_idx: usize },
}

/// Simple LCS-based diff. O(nm) space and time -- fine for snapshots (<1000 lines).
#[expect(
    clippy::indexing_slicing,
    reason = "LCS table indexed by loop-bounded i,j"
)]
fn lcs(old: &[String], new: &[String]) -> Vec<DiffOp> {
    let m = old.len();
    let n = new.len();

    // Build LCS table
    let mut table = vec![vec![0_usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old[i - 1] == new[j - 1] {
                table[i][j] = table[i - 1][j - 1] + 1;
            } else {
                table[i][j] = table[i - 1][j].max(table[i][j - 1]);
            }
        }
    }

    // Backtrack to produce diff operations
    let mut ops: Vec<DiffOp> = Vec::new();
    let mut i = m;
    let mut j = n;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            ops.push(DiffOp::Keep { new_idx: j - 1 });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
            ops.push(DiffOp::Insert { new_idx: j - 1 });
            j -= 1;
        } else {
            ops.push(DiffOp::Delete { old_idx: i - 1 });
            i -= 1;
        }
    }

    ops.reverse();
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_single_ref() {
        let input = r#"  button @e5 "OK" (100,200 80x30)"#;
        let expected = r#"  button "OK" (100,200 80x30)"#;
        assert_eq!(strip_refs(input), expected);
    }

    #[test]
    fn strips_ref_at_end() {
        assert_eq!(strip_refs("  button @e12"), "  button");
    }

    #[test]
    fn leaves_non_ref_lines() {
        let input = "  group \"Settings\"";
        assert_eq!(strip_refs(input), input);
    }

    #[test]
    fn strips_large_ref() {
        assert_eq!(
            strip_refs("  menuitem @e302 \"Paste\""),
            "  menuitem \"Paste\""
        );
    }

    #[test]
    fn strips_multiple_refs() {
        assert_eq!(strip_refs("  row @e1 @e2 @e3 item"), "  row item");
    }

    #[test]
    fn strips_ref_without_trailing_space() {
        // Ref at end of line, no trailing space to consume
        assert_eq!(strip_refs("button @e99"), "button");
    }

    #[test]
    fn strips_ref_followed_by_non_space() {
        // "@e5" immediately followed by a quote -- no space consumed
        assert_eq!(strip_refs("@e5\"label\""), "\"label\"");
    }

    #[test]
    fn no_match_without_digits() {
        // "@e" with no digit after it is not a ref
        assert_eq!(strip_refs("text @e other"), "text @e other");
    }

    #[test]
    fn no_match_email_address() {
        // "user@example" has @ but no "@e" prefix
        assert_eq!(strip_refs("user@example.com"), "user@example.com");
    }

    #[test]
    fn empty_string() {
        assert_eq!(strip_refs(""), "");
    }

    #[test]
    fn only_a_ref() {
        assert_eq!(strip_refs("@e42"), "");
    }

    #[test]
    fn only_refs_and_spaces() {
        assert_eq!(strip_refs("@e1 @e2 @e3 "), "");
    }

    #[test]
    fn preserves_internal_spaces() {
        // Two spaces before ref, ref consumes @e5 + one trailing space, one space remains after
        assert_eq!(strip_refs("  button  @e5  label"), "  button   label");
    }

    #[test]
    fn preserves_multibyte_chars() {
        // Emoji and CJK characters in the name should survive intact
        assert_eq!(
            strip_refs("  button @e1 \"\u{1F389} Settings \u{8BBE}\u{7F6E}\""),
            "  button \"\u{1F389} Settings \u{8BBE}\u{7F6E}\""
        );
    }

    #[test]
    fn identical_snapshots_no_changes() {
        let text =
            "app: Finder\n  window \"Home\"\n    button @e1 \"Close\"\n    button @e2 \"Minimize\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(text, text);
        assert!(diff.is_empty());
        assert!(diff.added().is_empty());
        assert!(diff.removed().is_empty());
        assert_eq!(diff.summary(), "no changes");
    }

    #[test]
    fn detects_added() {
        let old = "app: TestApp\n  window \"Main\"\n    button @e1 \"OK\"";
        let new =
            "app: TestApp\n  window \"Main\"\n    button @e1 \"OK\"\n    button @e2 \"Cancel\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        assert_eq!(diff.added().len(), 1);
        assert!(diff.removed().is_empty());
        assert!(diff.added()[0].text.contains("Cancel"));
    }

    #[test]
    fn detects_removed() {
        let old =
            "app: TestApp\n  window \"Main\"\n    button @e1 \"OK\"\n    button @e2 \"Cancel\"";
        let new = "app: TestApp\n  window \"Main\"\n    button @e1 \"OK\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        assert_eq!(diff.removed().len(), 1);
        assert!(diff.added().is_empty());
        assert!(diff.removed()[0].text.contains("Cancel"));
    }

    #[test]
    fn ref_shift_handled() {
        let old = "app: TestApp\n  window \"Main\"\n    button @e1 \"Save\"\n    textfield @e2 \"Name\"\n    button @e3 \"Cancel\"";
        let new = "app: TestApp\n  window \"Main\"\n    button @e1 \"New\"\n    button @e2 \"Save\"\n    textfield @e3 \"Name\"\n    button @e4 \"Cancel\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        assert_eq!(diff.added().len(), 1);
        assert!(diff.removed().is_empty());
        assert!(diff.added()[0].text.contains("New"));
        let unchanged: Vec<_> = diff.unchanged().iter().map(|l| l.text.clone()).collect();
        assert!(unchanged
            .iter()
            .any(|t| t.contains("@e2") && t.contains("Save")));
        assert!(unchanged
            .iter()
            .any(|t| t.contains("@e3") && t.contains("Name")));
        assert!(unchanged
            .iter()
            .any(|t| t.contains("@e4") && t.contains("Cancel")));
    }

    #[test]
    fn detects_value_change() {
        let old = "app: TestApp\n  window \"Main\"\n    textfield @e1 \"Search\" value=\"hello\"";
        let new =
            "app: TestApp\n  window \"Main\"\n    textfield @e1 \"Search\" value=\"hello world\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        assert_eq!(diff.removed().len(), 1);
        assert_eq!(diff.added().len(), 1);
        assert!(diff.removed()[0].text.contains("hello"));
        assert!(diff.added()[0].text.contains("hello world"));
    }

    #[test]
    fn mixed_changes() {
        let old =
            "app: TestApp\n  window \"Main\"\n    button @e1 \"Submit\"\n    button @e2 \"Reset\"";
        let new = "app: TestApp\n  window \"Main\"\n    button @e1 \"Submit\"\n    button @e2 \"Cancel\"\n    link @e3 \"Help\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        assert_eq!(diff.removed().len(), 1); // Reset
        assert_eq!(diff.added().len(), 2); // Cancel + Help
        assert!(diff.removed()[0].text.contains("Reset"));
    }

    #[test]
    fn empty_old() {
        let old = "app: TestApp";
        let new = "app: TestApp\n  button @e1 \"OK\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        assert_eq!(diff.added().len(), 1);
        assert!(diff.removed().is_empty());
    }

    #[test]
    fn empty_new() {
        let old = "app: TestApp\n  button @e1 \"OK\"";
        let new = "app: TestApp";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        assert_eq!(diff.removed().len(), 1);
        assert!(diff.added().is_empty());
    }

    #[test]
    fn render_markers() {
        let old = "app: TestApp\n  button @e1 \"OK\"";
        let new = "app: TestApp\n  button @e1 \"OK\"\n  button @e2 \"Cancel\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        let output = diff.render(0);
        assert!(output.contains('+'));
        assert!(output.contains("Cancel"));
        assert!(output.contains("[diff:"));
    }

    #[test]
    fn render_with_context() {
        let old = "app: TestApp\n  window \"Main\"\n    group \"A\"\n      button @e1 \"One\"\n    group \"B\"\n      button @e2 \"Two\"\n    group \"C\"\n      button @e3 \"Three\"";
        let new = "app: TestApp\n  window \"Main\"\n    group \"A\"\n      button @e1 \"One\"\n    group \"B\"\n      button @e2 \"Two\"\n      button @e3 \"New\"\n    group \"C\"\n      button @e4 \"Three\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        let output = diff.render(1);
        assert!(output.contains("  ")); // unchanged lines
        assert!(output.contains("+ ")); // added line
    }

    #[test]
    fn render_no_changes() {
        let text = "app: TestApp\n  button @e1 \"OK\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(text, text);
        let output = diff.render(0);
        assert_eq!(output, "[no changes]");
    }

    #[test]
    fn element_moved() {
        let old = "app: TestApp\n  group \"A\"\n    button @e1 \"OK\"\n  group \"B\"";
        let new = "app: TestApp\n  group \"A\"\n  group \"B\"\n    button @e1 \"OK\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        assert_eq!(diff.added().len(), 1);
        assert_eq!(diff.removed().len(), 1);
    }

    #[test]
    fn summary_describes_changes() {
        let old = "app: TestApp\n  button @e1 \"A\"\n  button @e2 \"B\"";
        let new = "app: TestApp\n  button @e1 \"A\"\n  button @e2 \"C\"\n  button @e3 \"D\"";
        let differ = SnapshotDiffer::new();
        let diff = differ.diff(old, new);
        let summary = diff.summary();
        assert!(summary.contains("added"));
        assert!(summary.contains("removed"));
    }
}
