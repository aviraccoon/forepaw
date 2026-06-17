//! Shared CF type conversion helpers for the Darwin backend.
//!
//! Provides safe-ish wrappers around common CF type extraction patterns:
//! reading values from `CFDictionary`, converting `CGColor` to hex strings,
//! parsing `CFBoolean`/`CFNumber` as Rust `bool`, etc.
//!
//! All functions are `unsafe` because they dereference raw CF pointers.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2_foundation::NSString;

use super::ffi::{
    kCFNull, CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef, CFDictionaryGetTypeID,
    CFDictionaryGetValue, CFDictionaryRef, CFGetTypeID, CFIndex, CFNumberGetTypeID,
    CFNumberGetValue, CFNumberRef, CFStringGetCString, CFStringGetCStringPtr, CFStringGetLength,
    CFStringGetTypeID, CFStringRef, CFTypeRef, CGColorGetComponents, CGColorGetNumberOfComponents,
    CGColorGetTypeID, CGColorRef, K_CF_NUMBER_DOUBLE_TYPE, K_CF_NUMBER_SINT32_TYPE,
    K_CF_STRING_ENCODING_UTF8,
};

/// Convert a `CFStringRef` to a Rust String. Handles both ASCII (fast path)
/// and non-ASCII (buffer copy) strings.
pub(super) fn cf_string_to_rust(cf_str: CFStringRef) -> Option<String> {
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "CFString fast ptr + slow buffer"
    )]
    // SAFETY: CFString conversion on valid CFStringRef.
    unsafe {
        // Fast path: ASCII/null-fast strings
        let ptr = CFStringGetCStringPtr(cf_str, K_CF_STRING_ENCODING_UTF8);
        if !ptr.is_null() {
            return std::ffi::CStr::from_ptr(ptr)
                .to_str()
                .ok()
                .map(String::from);
        }
        // Slow path: non-ASCII strings need a buffer copy
        let len = CFStringGetLength(cf_str);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "CFString length fits in usize"
        )]
        #[expect(clippy::cast_sign_loss, reason = "CFString length is non-negative")]
        let len_usize = len as usize;
        let mut buf = vec![0_u8; (len_usize + 1) * 4]; // worst case: 4 bytes per char
        #[expect(
            clippy::cast_possible_wrap,
            reason = "buffer length fits in CFIndex (i64)"
        )]
        let buf_len = buf.len() as CFIndex;
        if CFStringGetCString(
            cf_str,
            buf.as_mut_ptr().cast::<std::ffi::c_char>(),
            buf_len,
            K_CF_STRING_ENCODING_UTF8,
        ) {
            // Use CStr to find the actual null-terminated length
            let actual_len = std::ffi::CStr::from_ptr(buf.as_ptr().cast::<std::ffi::c_char>())
                .to_bytes()
                .len();
            buf.truncate(actual_len);
            String::from_utf8(buf).ok()
        } else {
            None
        }
    }
}

/// Extract a value from a `CFDictionary` by string key.
///
/// Creates a temporary `CFString` for the key lookup and releases it.
/// Returns `None` if the key is missing or the value is `kCFNull`.
///
/// # Safety
///
/// `dict` must be a valid, non-null `CFDictionaryRef`.
pub(super) unsafe fn get_dict_value(dict: CFDictionaryRef, key: &str) -> Option<CFTypeRef> {
    let cf_key = cf_string_from_str(key);
    let val = CFDictionaryGetValue(dict, cf_key.cast());
    super::ffi::CFRelease(cf_key as CFTypeRef);
    if val.is_null() || val == kCFNull.cast::<c_void>() {
        return None;
    }
    Some(val as CFTypeRef)
}

/// Get a string value from a `CFDictionary` by key.
///
/// # Safety
///
/// `dict` must be a valid, non-null `CFDictionaryRef`.
pub(super) unsafe fn get_dict_string(dict: CFDictionaryRef, key: &str) -> Option<String> {
    let val = get_dict_value(dict, key)?;
    if CFGetTypeID(val) != CFStringGetTypeID() {
        return None;
    }
    cf_string_to_rust(val as CFStringRef)
}

/// Get an f64 value from a `CFDictionary` by key.
///
/// # Safety
///
/// `dict` must be a valid, non-null `CFDictionaryRef`.
pub(super) unsafe fn get_dict_f64(dict: CFDictionaryRef, key: &str) -> Option<f64> {
    let val = get_dict_value(dict, key)?;
    if CFGetTypeID(val) != CFNumberGetTypeID() {
        return None;
    }
    let mut float_val: f64 = 0.0;
    if CFNumberGetValue(
        val as CFNumberRef,
        K_CF_NUMBER_DOUBLE_TYPE,
        (&raw mut float_val).cast::<c_void>(),
    ) == 0
    {
        return None;
    }
    Some(float_val)
}

/// Convert a `CFTypeRef` to a bool, handling both `CFBoolean` and `CFNumber`.
///
/// Boolean is the primary type for strikethrough/shadow/misspelled. Number
/// is used for underline style (`AXUnderlineStyle`, where nonzero = true).
///
/// # Safety
///
/// `val` must be a valid `CFTypeRef` or null.
pub(super) unsafe fn cftype_to_bool(val: CFTypeRef) -> Option<bool> {
    let type_id = CFGetTypeID(val);
    if type_id == CFBooleanGetTypeID() {
        let result = CFBooleanGetValue(val as CFBooleanRef);
        Some(result != 0)
    } else if type_id == CFNumberGetTypeID() {
        let mut int_val: i64 = 0;
        if CFNumberGetValue(
            val as CFNumberRef,
            K_CF_NUMBER_SINT32_TYPE,
            (&raw mut int_val).cast::<c_void>(),
        ) == 0
        {
            return None;
        }
        Some(int_val != 0)
    } else {
        None
    }
}

/// Create a `CFString` from a Rust &str. Caller must `CFRelease` the result.
#[must_use]
pub fn cf_string_from_str(s: &str) -> CFStringRef {
    // NSString::from_str creates an autoreleased string.
    // We retain it manually so the caller owns it.
    let ns = NSString::from_str(s);
    let ptr = Retained::as_ptr(&ns) as CFStringRef;
    // Prevent Drop from releasing -- caller takes ownership via CFRelease
    #[expect(
        clippy::mem_forget,
        reason = "transfer ownership to caller via CFRelease"
    )]
    std::mem::forget(ns);
    ptr
}

/// Convert a `CFNumberRef` to a Rust String. Tries integer first, then float.
pub(super) fn number_to_rust_string(number: CFNumberRef) -> Option<String> {
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "CFNumber type dispatch"
    )]
    // SAFETY: CFNumberGetValue reads from valid CFNumber.
    unsafe {
        // Try as i64 first (most AX values are integers)
        let mut val: i64 = 0;
        // K_CF_NUMBER_SINT64_TYPE = 4 (not in our FFI, use raw value)
        if CFNumberGetValue(number, 4, (&raw mut val).cast::<c_void>()) != 0 {
            return Some(val.to_string());
        }
        // Fallback: f64
        let mut fval: f64 = 0.0;
        if CFNumberGetValue(
            number,
            K_CF_NUMBER_DOUBLE_TYPE,
            (&raw mut fval).cast::<c_void>(),
        ) != 0
        {
            return Some(if (fval - fval.floor()).abs() < f64::EPSILON {
                format!("{fval:.0}")
            } else {
                format!("{fval}")
            });
        }
        None
    }
}

/// Convert a `CGColorRef` to a hex string, or return None.
///
/// # Safety
///
/// `val` must be a valid `CFTypeRef` or null.
pub(super) unsafe fn cgcolor_hex_or_none(val: CFTypeRef) -> Option<String> {
    if CFGetTypeID(val) != CGColorGetTypeID() {
        return None;
    }
    cgcolor_to_hex(val as CGColorRef)
}

/// Convert a `CGColorRef` to a hex string (`#RRGGBB` or `#RRGGBBAA`).
///
/// # Safety
///
/// `color` must be a valid, non-null `CGColorRef`.
pub(super) unsafe fn cgcolor_to_hex(color: CGColorRef) -> Option<String> {
    let components = CGColorGetComponents(color);
    let count = CGColorGetNumberOfComponents(color);
    let (r, g, b, a) = match count {
        2 => {
            let gray = *components;
            let alpha = *components.add(1);
            (gray, gray, gray, alpha)
        }
        4 => (
            *components,
            *components.add(1),
            *components.add(2),
            *components.add(3),
        ),
        _ => return None,
    };
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ri = (r.clamp(0.0, 1.0) * 255.0).round() as u32;
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let gi = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bi = (b.clamp(0.0, 1.0) * 255.0).round() as u32;
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ai = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
    if ai < 255 {
        Some(format!("#{ri:02x}{gi:02x}{bi:02x}{ai:02x}"))
    } else {
        Some(format!("#{ri:02x}{gi:02x}{bi:02x}"))
    }
}

/// Get an i32 value from a `CFNumberRef`.
///
/// Used for attributes like `AXSuperscript` that carry small signed integers.
///
/// # Safety
///
/// `val` must be a valid `CFTypeRef`.
pub(super) unsafe fn get_dict_i32(val: CFTypeRef) -> Option<i32> {
    if CFGetTypeID(val) != CFNumberGetTypeID() {
        return None;
    }
    let mut int_val: i32 = 0;
    if CFNumberGetValue(
        val as CFNumberRef,
        K_CF_NUMBER_SINT32_TYPE,
        (&raw mut int_val).cast::<c_void>(),
    ) == 0
    {
        return None;
    }
    Some(int_val)
}

// ---------------------------------------------------------------------------
// CFStringRef-keyed dictionary helpers (for CGWindowListCopyWindowInfo dicts)
//
// These take pre-existing CFStringRef keys (from FFI constants like kCGWindowOwnerName)
// rather than Rust &str keys that need temporary CFString allocation.
// ---------------------------------------------------------------------------

/// Get a string value from a `CFDictionary` by a `CFStringRef` key.
///
/// Used with `CGWindowListCopyWindowInfo` dictionaries whose keys are pre-existing
/// `CFString` constants (`kCGWindowOwnerName`, `kCGWindowName`, etc.).
///
/// # Safety
///
/// `dict` must be a valid, non-null `CFDictionaryRef`.
/// `key` must be a valid `CFStringRef`.
pub(super) unsafe fn get_dict_string_ref(
    dict: CFDictionaryRef,
    key: CFStringRef,
) -> Option<String> {
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "multiple FFI calls")]
    // SAFETY: FFI calls on valid CoreFoundation objects.
    unsafe {
        let val = CFDictionaryGetValue(dict, key.cast::<c_void>());
        if val.is_null() {
            return None;
        }
        if CFGetTypeID(val as CFTypeRef) != CFStringGetTypeID() {
            return None;
        }
        // Fast path: null-fast strings (ASCII/UTF-8 inline)
        let ptr = CFStringGetCStringPtr(val as CFStringRef, K_CF_STRING_ENCODING_UTF8);
        if !ptr.is_null() {
            return std::ffi::CStr::from_ptr(ptr)
                .to_str()
                .ok()
                .map(String::from);
        }
        // Slow path: buffer copy for non-ASCII
        let mut buf = [0_u8; 1024];
        #[expect(
            clippy::cast_possible_wrap,
            reason = "buffer length fits in CFIndex (i64)"
        )]
        let buf_len = buf.len() as CFIndex;
        if CFStringGetCString(
            val as CFStringRef,
            buf.as_mut_ptr().cast::<std::ffi::c_char>(),
            buf_len,
            K_CF_STRING_ENCODING_UTF8,
        ) {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            buf.get(..len)
                .and_then(|slice| std::str::from_utf8(slice).ok())
                .map(String::from)
        } else {
            None
        }
    }
}

/// Get an i32 value from a `CFDictionary` by a `CFStringRef` key.
///
/// # Safety
///
/// `dict` must be a valid, non-null `CFDictionaryRef`.
/// `key` must be a valid `CFStringRef`.
pub(super) unsafe fn get_dict_i32_ref(dict: CFDictionaryRef, key: CFStringRef) -> Option<i32> {
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "multiple FFI calls")]
    // SAFETY: FFI calls on valid CoreFoundation objects.
    unsafe {
        let val = CFDictionaryGetValue(dict, key.cast::<c_void>());
        if val.is_null() {
            return None;
        }
        if CFGetTypeID(val as CFTypeRef) != CFNumberGetTypeID() {
            return None;
        }
        let mut result: i32 = 0;
        if CFNumberGetValue(
            val as CFNumberRef,
            K_CF_NUMBER_SINT32_TYPE,
            (&raw mut result).cast::<c_void>(),
        ) != 0
        {
            Some(result)
        } else {
            None
        }
    }
}

/// Get a `Rect` from a `CFDictionary` by a `CFStringRef` key.
///
/// The value is expected to be a nested `CFDictionary` with `X`, `Y`,
/// `Width`, `Height` entries (as used by `kCGWindowBounds`).
///
/// # Safety
///
/// `dict` must be a valid, non-null `CFDictionaryRef`.
/// `key` must be a valid `CFStringRef`.
pub(super) unsafe fn get_dict_bounds_ref(
    dict: CFDictionaryRef,
    key: CFStringRef,
) -> Option<crate::core::types::Rect> {
    use crate::core::types::Rect;
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "multiple FFI calls")]
    // SAFETY: FFI calls on valid CoreFoundation objects.
    unsafe {
        let val = CFDictionaryGetValue(dict, key.cast::<c_void>());
        if val.is_null() {
            return None;
        }
        if CFGetTypeID(val as CFTypeRef) != CFDictionaryGetTypeID() {
            return None;
        }
        let bounds_dict = val as CFDictionaryRef;
        let x = get_dict_f64(bounds_dict, "X").unwrap_or(0.0);
        let y = get_dict_f64(bounds_dict, "Y").unwrap_or(0.0);
        let w = get_dict_f64(bounds_dict, "Width").unwrap_or(0.0);
        let h = get_dict_f64(bounds_dict, "Height").unwrap_or(0.0);
        Some(Rect {
            x,
            y,
            width: w,
            height: h,
        })
    }
}

/// Get an integer attribute from an AX element.
///
/// # Safety
///
/// `element` must be a valid, non-null `AXUIElementRef`.
pub(super) unsafe fn get_ax_int_count(
    element: super::ffi::AXUIElementRef,
    attr: &str,
) -> Option<CFIndex> {
    use super::ffi::AXError;
    let cf_attr = cf_string_from_str(attr);
    let mut value: CFTypeRef = std::ptr::null_mut();
    let err = super::ffi::AXUIElementCopyAttributeValue(element, cf_attr, &raw mut value);
    super::ffi::CFRelease(cf_attr as CFTypeRef);
    if err != AXError::Success || value.is_null() {
        return None;
    }
    if CFGetTypeID(value) != CFNumberGetTypeID() {
        super::ffi::CFRelease(value);
        return None;
    }
    let mut int_val: i64 = 0;
    let ok = CFNumberGetValue(
        value as CFNumberRef,
        K_CF_NUMBER_SINT32_TYPE,
        (&raw mut int_val).cast::<c_void>(),
    );
    super::ffi::CFRelease(value);
    if ok == 0 {
        return None;
    }
    Some(int_val)
}
