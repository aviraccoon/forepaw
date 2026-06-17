//! Text formatting attributes extracted from accessible text elements.
//!
//! Provides font, color, and decoration information from platform-specific
//! text attribute APIs. The [`TextAttributes`] struct is platform-agnostic;
//! each platform backend translates its native text attribute format into
//! this common representation.
//!
//! Text with mixed formatting (bold highlights, colored spans) is exposed as
//! multiple [`TextAttrsRun`] entries inside [`TextAttrsResult`], each with its
//! own [`TextAttributes`] and character range.

/// Text formatting attributes for accessible text elements.
///
/// Provides font, color, and decoration information for a single style run.
/// Returned as part of [`TextAttrsResult`] from
/// [`crate::platform::DesktopProvider::get_text_attributes`].
///
/// Available on macOS (via `AXAttributedStringForRange`), Windows (via
/// UIA `TextPattern.GetAttributeValue`), and Linux (via AT-SPI2
/// `Text.GetAttributes`). Each platform supports a subset of these
/// fields. Unsupported fields are `None` (or `false` for booleans).
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct TextAttributes {
    /// Font family, e.g. `"Helvetica"`, `".AppleSystemUIFont"`.
    pub font_family: Option<String>,
    /// Font postscript name, e.g. `"Helvetica-Bold"`, `".SFNS-Regular"`.
    pub font_name: Option<String>,
    /// Human-readable font display name, e.g. `"System Font Regular"`.
    pub font_visible_name: Option<String>,
    /// Font size in points.
    pub font_size: Option<f64>,
    /// Foreground color as a hex string (`#RRGGBB` or `#RRGGBBAA`).
    pub foreground_color: Option<String>,
    /// Background color as a hex string (`#RRGGBB` or `#RRGGBBAA`).
    pub background_color: Option<String>,
    /// Whether the text has strikethrough decoration.
    /// `None` means the attribute is not available on this element/platform.
    pub strikethrough: Option<bool>,
    /// Whether the text has underline decoration.
    /// `None` means the attribute is not available on this element/platform.
    pub underline: Option<bool>,
    /// Color of the underline decoration as a hex string (`#RRGGBB` or `#RRGGBBAA`).
    /// Available on macOS (`AXUnderlineColor`). Falls back to the foreground color on
    /// platforms that don't expose underline color separately.
    pub underline_color: Option<String>,
    /// Color of the strikethrough decoration as a hex string (`#RRGGBB` or `#RRGGBBAA`).
    /// Available on macOS (`AXStrikethroughColor`).
    pub strikethrough_color: Option<String>,
    /// Superscript/subscript level. Positive = superscript, negative = subscript,
    /// zero = baseline.
    pub superscript: Option<i32>,
    /// Whether the text has a shadow.
    pub shadow: Option<bool>,
    /// Natural language tag for the text (e.g. `"en"`, `"fr"`).
    pub natural_language: Option<String>,
}
/// A contiguous range of characters with uniform text formatting.
///
/// Part of [`TextAttrsResult`] — text with mixed formatting is split into
/// multiple runs, each with its own [`TextAttributes`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextAttrsRun {
    /// Start character index (0-based, platform-native encoding).
    pub start: usize,
    /// Length in characters (platform-native encoding).
    pub length: usize,
    /// Font, color, and decoration attributes for this run.
    pub attrs: TextAttributes,
}

/// Result of a text attribute query, containing per-run attribute breakdown.
///
/// Returned by [`crate::platform::DesktopProvider::get_text_attributes`].
/// For uniform text the `runs` vec has a single entry; for mixed-format
/// text there is one entry per formatting change across the text content.
#[derive(Debug, Clone, PartialEq)]
pub struct TextAttrsResult {
    /// The text content (limited to the queried range, if applicable).
    pub text: Option<String>,
    /// Per-style-run attribute breakdown.
    pub runs: Vec<TextAttrsRun>,
}
