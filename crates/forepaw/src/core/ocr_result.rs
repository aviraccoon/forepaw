//! OCR result types.
use crate::core::types::Rect;

/// Recognized text with its bounding box.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OCRResult {
    /// The recognized text.
    pub text: String,
    /// Bounding box of the recognized text region.
    pub bounds: Rect,
}

impl OCRResult {
    /// Create a new OCR result.
    pub fn new(text: impl Into<String>, bounds: Rect) -> Self {
        Self {
            text: text.into(),
            bounds,
        }
    }

    /// Center point of the recognized text region.
    #[must_use]
    pub fn center(&self) -> (f64, f64) {
        (
            self.bounds.x + self.bounds.width / 2.0,
            self.bounds.y + self.bounds.height / 2.0,
        )
    }
}

/// Find all case-insensitive substring matches in `text`, returning their UTF-8 byte ranges.
///
/// The returned `(start, end)` pairs are UTF-8 byte offsets suitable for slicing `text`
/// with `text[start..end]`. They are NOT UTF-16 code unit offsets — convert when
/// interfacing with NSString/NSRange APIs.
///
/// Walks `text` char-by-char with case-folded comparison to avoid the Unicode
/// length-change pitfall of `str::to_lowercase()` + slicing the original string.
#[must_use]
pub fn find_case_insensitive_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let q_lower: Vec<char> = query
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let t_lower: Vec<char> = text
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();

    let mut ranges = Vec::new();
    let mut i = 0;
    while i + q_lower.len() <= t_lower.len() {
        if t_lower.get(i..).is_some_and(|s| s.starts_with(&q_lower)) {
            let byte_start = char_indices.get(i).map_or(0, |&(pos, _)| pos);
            let byte_end = char_indices
                .get(i + q_lower.len())
                .map_or(text.len(), |&(pos, _)| pos);
            ranges.push((byte_start, byte_end));
            i += q_lower.len();
        } else {
            i += 1;
        }
    }
    ranges
}

/// Combined OCR output: recognized text results plus optional display screenshot.
#[derive(Debug, Clone, serde::Serialize)]
#[non_exhaustive]
pub struct OCROutput {
    /// All recognized text results.
    pub results: Vec<OCRResult>,
    /// Path to the screenshot used for OCR, if saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_path: Option<String>,
}

impl OCROutput {
    /// Create new OCR output.
    #[must_use]
    pub fn new(results: Vec<OCRResult>, screenshot_path: Option<String>) -> Self {
        Self {
            results,
            screenshot_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_result_center() {
        let r = OCRResult::new("Hello", Rect::new(10.0, 20.0, 50.0, 15.0));
        let (cx, cy) = r.center();
        assert!((cx - 35.0).abs() < f64::EPSILON);
        assert!((cy - 27.5).abs() < f64::EPSILON);
    }

    #[test]
    fn ocr_output_with_results_and_path() {
        let output = OCROutput::new(
            vec![OCRResult::new("Hello", Rect::new(10.0, 20.0, 50.0, 15.0))],
            Some("/tmp/test.jpg".into()),
        );
        assert_eq!(output.results.len(), 1);
        assert_eq!(output.screenshot_path.as_deref(), Some("/tmp/test.jpg"));
    }

    #[test]
    fn ocr_output_empty() {
        let output = OCROutput::new(Vec::new(), None);
        assert!(output.results.is_empty());
        assert!(output.screenshot_path.is_none());
    }

    #[test]
    fn find_ranges_simple() {
        let ranges = find_case_insensitive_ranges("Hello World", "world");
        assert_eq!(ranges, vec![(6, 11)]);
    }

    #[test]
    fn find_ranges_case_insensitive() {
        let ranges = find_case_insensitive_ranges("Hello World", "WORLD");
        assert_eq!(ranges, vec![(6, 11)]);
    }

    #[test]
    fn find_ranges_multiple() {
        let ranges = find_case_insensitive_ranges("test test test", "test");
        assert_eq!(ranges, vec![(0, 4), (5, 9), (10, 14)]);
    }

    #[test]
    fn find_ranges_ascii_inside_multi_byte() {
        // "é" is 2 bytes; "test" after it should have correct byte offsets
        let ranges = find_case_insensitive_ranges("é test", "TEST");
        assert_eq!(ranges, vec![(3, 7)]);
    }

    #[test]
    fn find_ranges_no_match() {
        let ranges = find_case_insensitive_ranges("Hello World", "xyz");
        assert!(ranges.is_empty());
    }

    #[test]
    fn find_ranges_empty_query() {
        let ranges = find_case_insensitive_ranges("Hello", "");
        assert!(ranges.is_empty());
    }

    #[test]
    fn find_ranges_overlapping_not_matched() {
        // "aaa" in "aaaa": only "aaa" at 0-3, then 3-6 (not overlapping)
        let ranges = find_case_insensitive_ranges("aaaa", "aaa");
        assert_eq!(ranges, vec![(0, 3)]);
    }

    #[test]
    fn find_ranges_cjk() {
        // Each CJK character is 3 bytes
        let ranges = find_case_insensitive_ranges("狸猫浣熊", "浣熊");
        assert_eq!(ranges, vec![(6, 12)]);
    }

    #[expect(
        clippy::string_slice,
        reason = "byte_offset is a validated UTF-8 boundary from find_case_insensitive_ranges"
    )]
    #[test]
    fn byte_range_to_utf16_offset_acii() {
        let text = "hello world";
        // byte 6 == UTF-16 offset 6 (ASCII is 1:1)
        let byte_offset = 6;
        let utf16_off: usize = text[..byte_offset].chars().map(char::len_utf16).sum();
        assert_eq!(utf16_off, 6);
    }

    #[expect(
        clippy::string_slice,
        reason = "byte_offset is a validated UTF-8 boundary from find_case_insensitive_ranges"
    )]
    #[test]
    fn byte_range_to_utf16_offset_multibyte() {
        let text = "é test"; // é is 2 bytes in UTF-8, 1 code unit in UTF-16
                             // byte offset 3 = char index 2 (skip é + space)
        let byte_offset = 3;
        let utf16_off: usize = text[..byte_offset].chars().map(char::len_utf16).sum();
        // é is 2 UTF-8 bytes but 1 UTF-16 code unit, space is 1:1
        assert_eq!(utf16_off, 2); // 1 (é) + 1 (space) = 2
    }

    #[expect(
        clippy::string_slice,
        reason = "byte_offset is a validated UTF-8 boundary from find_case_insensitive_ranges"
    )]
    #[test]
    fn byte_range_to_utf16_offset_emoji() {
        // "🦝" is 4 bytes in UTF-8 but 2 UTF-16 code units (surrogate pair)
        let text = "🦝 test";
        // byte offset 5 = after 🦝 + space
        let byte_offset = 5;
        let utf16_off: usize = text[..byte_offset].chars().map(char::len_utf16).sum();
        // 🦝 is 2 UTF-16 code units, space is 1, total = 3
        assert_eq!(utf16_off, 3);
    }

    #[expect(
        clippy::string_slice,
        reason = "byte_start/byte_end are validated UTF-8 boundaries from find_case_insensitive_ranges"
    )]
    #[test]
    fn find_ranges_utf16_mismatch_demonstrated() {
        // Verifies that UTF-8 byte offsets and UTF-16 code unit offsets
        // differ for non-ASCII text. This is why find_precise_matches must
        // convert before constructing NSRange.
        let ranges = find_case_insensitive_ranges("é test", "test");
        assert_eq!(ranges, vec![(3, 7)]); // byte ranges

        let text = "é test";
        let (byte_start, byte_end) = ranges[0];
        let utf16_start: usize = text[..byte_start].chars().map(char::len_utf16).sum();
        let utf16_len: usize = text[byte_start..byte_end]
            .chars()
            .map(char::len_utf16)
            .sum();

        // Byte offset 3 → UTF-16 offset 2 (é=1 + space=1)
        assert_eq!(utf16_start, 2);
        assert_eq!(utf16_len, 4); // "test" is all ASCII, 4 UTF-16 code units

        // UTF-8 byte offset must differ from UTF-16 for non-ASCII prefixes
        assert_ne!(byte_start, utf16_start);
    }
}
