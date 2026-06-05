//! Checked cast helpers for FFI boundary values.
//!
//! Platform APIs return values in types that don't match what consumers expect
//! (e.g. `usize` image dimensions → `i32` FFI parameters). These helpers
//! validate at the boundary and produce clear errors instead of silent truncation.

use crate::core::errors::ForepawError;

/// Narrow `usize` to `i32` for FFI API parameters.
///
/// Used where a platform API expects a signed integer but the source
/// is an unsigned Rust value (e.g. `CGImage` pixel dimensions).
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `value` exceeds `i32::MAX`.
pub fn usize_to_i32(value: usize) -> Result<i32, ForepawError> {
    i32::try_from(value)
        .map_err(|_| ForepawError::ActionFailed(format!("value {value} exceeds i32::MAX")))
}

/// Narrow `i64` to `i32` for crop rectangle coordinates.
///
/// Used where crop coordinates are computed as `i64` but the image
/// API expects `i32`.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `value` exceeds `i32` range.
pub fn i64_to_i32(value: i64) -> Result<i32, ForepawError> {
    i32::try_from(value)
        .map_err(|_| ForepawError::ActionFailed(format!("i64 value {value} exceeds i32 range")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usize_to_i32_normal() {
        assert_eq!(usize_to_i32(100).unwrap(), 100);
    }

    #[test]
    fn usize_to_i32_rejects_overflow() {
        usize_to_i32(i32::MAX as usize + 1).unwrap_err();
    }

    #[test]
    fn i64_to_i32_normal() {
        assert_eq!(i64_to_i32(42).unwrap(), 42);
    }

    #[test]
    fn i64_to_i32_rejects_overflow() {
        i64_to_i32(i64::from(i32::MAX) + 1).unwrap_err();
    }

    #[test]
    fn i64_to_i32_rejects_underflow() {
        i64_to_i32(i64::from(i32::MIN) - 1).unwrap_err();
    }
}
