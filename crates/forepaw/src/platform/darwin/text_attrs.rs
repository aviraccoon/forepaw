//! macOS text attribute extraction via `AXAttributedStringForRange`.
//!
//! Text elements (`AXStaticText`, `AXTextArea`) expose font, color, and
//! decoration information through the parameterized attribute
//! `AXAttributedStringForRange`. This module parses the returned
//! `CFAttributedString` into per-run [`TextAttrsResult`].

use std::ffi::c_void;

use crate::core::text_attrs::{TextAttributes, TextAttrsResult, TextAttrsRun};

use super::cf_convert::{
    cf_string_from_str, cf_string_to_rust, cftype_to_bool, cgcolor_hex_or_none, get_ax_int_count,
    get_dict_f64, get_dict_i32, get_dict_string, get_dict_value,
};
use super::ffi::{
    AXError, AXUIElementCopyParameterizedAttributeValue, AXValueCreate, AXValueType,
    CFAttributedStringGetAttributes, CFAttributedStringGetString, CFAttributedStringGetTypeID,
    CFAttributedStringRef, CFDictionaryGetTypeID, CFDictionaryRef, CFGetTypeID, CFIndex, CFRange,
    CFStringGetLength, CFStringGetTypeID, CFStringRef, CFTypeRef,
};
use super::snapshot::get_ax_string_attr;

/// Extract text formatting attributes from an AX element.
///
/// The element must be an `AXStaticText` or `AXTextArea`. Returns `None`
/// for other element types or when text attributes are unavailable.
///
/// The result includes the text content and a per-run breakdown of font,
/// color, and decoration attributes.
///
/// # Safety
///
/// `element` must be a valid, non-null `AXUIElementRef`.
pub(super) unsafe fn get_text_attributes(
    element: super::ffi::AXUIElementRef,
) -> Option<TextAttrsResult> {
    let role = get_ax_string_attr(element, "AXRole")?;
    if role != "AXStaticText" && role != "AXTextArea" {
        return None;
    }

    let char_count = get_ax_int_count(element, "AXNumberOfCharacters")?;
    if char_count <= 0 {
        return None;
    }

    let mut range = CFRange {
        location: 0,
        length: char_count,
    };
    let range_value = AXValueCreate(AXValueType::CFRange, (&raw mut range).cast::<c_void>());
    if range_value.0.is_null() {
        return None;
    }

    let attr_name = cf_string_from_str("AXAttributedStringForRange");
    let mut attr_str: CFTypeRef = std::ptr::null_mut();
    let err = AXUIElementCopyParameterizedAttributeValue(
        element,
        attr_name,
        range_value.0 as CFTypeRef,
        &raw mut attr_str,
    );
    super::ffi::CFRelease(attr_name as CFTypeRef);
    super::ffi::CFRelease(range_value.0 as CFTypeRef);
    if err != AXError::Success || attr_str.is_null() {
        return None;
    }

    if CFGetTypeID(attr_str) != CFAttributedStringGetTypeID() {
        crate::debug!(
            "AXAttributedStringForRange returned unexpected type ID {}, expected {}",
            CFGetTypeID(attr_str),
            CFAttributedStringGetTypeID(),
        );
        super::ffi::CFRelease(attr_str);
        return None;
    }
    let result = parse_attributed_string(attr_str as CFAttributedStringRef);
    super::ffi::CFRelease(attr_str);
    result
}

/// Parse a `CFAttributedString` into per-run `TextAttrsResult`.
///
/// Iterates attribute runs via `CFAttributedStringGetAttributes` and
/// collects each run's font, color, and decoration attributes along
/// with its character range. Also extracts the raw text content.
///
/// # Safety
///
/// `attr_str` must be a valid, non-null `CFAttributedStringRef`.
unsafe fn parse_attributed_string(attr_str: CFAttributedStringRef) -> Option<TextAttrsResult> {
    let cf_string = CFAttributedStringGetString(attr_str);
    if cf_string.is_null() {
        return None;
    }

    // Use CFStringGetLength for character count — CFAttributedStringGetAttributes
    // uses character indices, not UTF-8 byte lengths. Using Rust's string.len()
    // would overrun for multi-byte characters (e.g. em dash → 3 bytes vs 1 char).
    let len = CFStringGetLength(cf_string);
    if len <= 0 {
        return None;
    }

    let mut runs: Vec<TextAttrsRun> = Vec::new();
    let mut pos: CFIndex = 0;

    while pos < len {
        let mut effective_range = CFRange {
            location: 0,
            length: 0,
        };
        let attrs_dict = CFAttributedStringGetAttributes(attr_str, pos, &raw mut effective_range);
        if attrs_dict.is_null() {
            pos += 1;
            continue;
        }

        let mut run_attrs = TextAttributes::default();
        fill_run_attrs(attrs_dict, &mut run_attrs);

        let run_len = effective_range.length.max(0);
        if run_len > 0 || !runs.is_empty() {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "CFIndex (i64) is always >= 0 after .max(0) and fits in usize"
            )]
            let start = effective_range.location.max(0) as usize;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "CFIndex (i64) is always >= 0 after .max(0) and fits in usize"
            )]
            let length = run_len as usize;
            runs.push(TextAttrsRun {
                start,
                length,
                attrs: run_attrs,
            });
        }

        pos = effective_range.location + effective_range.length;
    }

    let text = cf_string_to_rust(cf_string);

    Some(TextAttrsResult { text, runs })
}

/// Populate a `TextAttributes` from one attribute run's dictionary.
///
/// Sets all fields that are present in the dictionary. Fields not present
/// remain as their default `None`/`false`.
///
/// # Safety
///
/// `dict` must be a valid, non-null `CFDictionaryRef`.
unsafe fn fill_run_attrs(dict: CFDictionaryRef, result: &mut TextAttributes) {
    // Font (AXFont → nested dict with AXFontFamily/AXFontName/AXFontSize/AXVisibleName)
    if let Some(font_dict) = get_dict_value(dict, "AXFont") {
        if CFGetTypeID(font_dict) == CFDictionaryGetTypeID() {
            let font_dict = font_dict as CFDictionaryRef;
            if let Some(val) = get_dict_string(font_dict, "AXFontFamily") {
                result.font_family = Some(val);
            }
            if let Some(val) = get_dict_string(font_dict, "AXFontName") {
                result.font_name = Some(val);
            }
            if let Some(val) = get_dict_string(font_dict, "AXVisibleName") {
                result.font_visible_name = Some(val);
            }
            if let Some(val) = get_dict_f64(font_dict, "AXFontSize") {
                result.font_size = Some(val);
            }
        } else {
            crate::debug!(
                "AXFont attribute is not a dictionary (type ID {})",
                CFGetTypeID(font_dict),
            );
        }
    }

    // Foreground color — CGColorRef
    if let Some(val) = get_dict_value(dict, "AXForegroundColor") {
        if let Some(hex) = cgcolor_hex_or_none(val) {
            result.foreground_color = Some(hex);
        }
    }

    // Background color — CGColorRef
    if let Some(val) = get_dict_value(dict, "AXBackgroundColor") {
        if let Some(hex) = cgcolor_hex_or_none(val) {
            result.background_color = Some(hex);
        }
    }

    // Strikethrough — CFBooleanRef (the header says CFBooleanRef)
    if let Some(val) = get_dict_value(dict, "AXStrikethrough") {
        if let Some(b) = cftype_to_bool(val) {
            // cftype_to_bool handles both CFBoolean (strikethrough) and CFNumber
            result.strikethrough = Some(b);
        }
    }

    // Underline — CFNumberRef (AXUnderlineStyle, nonzero = underline)
    if let Some(val) = get_dict_value(dict, "AXUnderline") {
        if let Some(b) = cftype_to_bool(val) {
            // cftype_to_bool handles both CFBoolean and CFNumber
            result.underline = Some(b);
        }
    }

    // Underline color — CGColorRef
    if let Some(val) = get_dict_value(dict, "AXUnderlineColor") {
        if let Some(hex) = cgcolor_hex_or_none(val) {
            result.underline_color = Some(hex);
        }
    }

    // Strikethrough color — CGColorRef
    if let Some(val) = get_dict_value(dict, "AXStrikethroughColor") {
        if let Some(hex) = cgcolor_hex_or_none(val) {
            result.strikethrough_color = Some(hex);
        }
    }

    // Superscript — CFNumberRef (0 = baseline, positive = superscript, negative = subscript)
    if let Some(val) = get_dict_value(dict, "AXSuperscript") {
        if let Some(s) = get_dict_i32(val) {
            result.superscript = Some(s);
        }
    }

    // Shadow — CFBooleanRef
    if let Some(val) = get_dict_value(dict, "AXShadow") {
        if let Some(b) = cftype_to_bool(val) {
            result.shadow = Some(b);
        }
    }

    // Natural language — CFStringRef
    if let Some(val) = get_dict_value(dict, "AXNaturalLanguage") {
        if CFGetTypeID(val) == CFStringGetTypeID() {
            if let Some(lang) = cf_string_to_rust(val as CFStringRef) {
                result.natural_language = Some(lang);
            }
        }
    }
}

/// Convert a `CGColorRef` to a hex string (`#RRGGBB` or `#RRGGBBAA`) — re-exported
/// from `cf_convert` for use by callers that have a known `CGColorRef`.
#[cfg(test)]
mod tests {
    use crate::core::text_attrs::TextAttrsResult;

    /// Verify that the struct layout is correct for consumer use.
    #[test]
    fn text_attrs_result_layout() {
        let result = TextAttrsResult {
            text: None,
            runs: vec![],
        };
        assert!(result.runs.is_empty());
        assert!(result.text.is_none());
    }
}
