//! FNV-1a 64-bit element content signatures.
//!
//! Provides a deterministic, cross-build-stable content hash for element
//! identity. Used to match elements across snapshot calls (cross-snapshot
//! identity) — the same logical element with the same content produces the
//! same signature on every snapshot of the same app.

use crate::core::role::Role;

/// FNV-1a 64-bit offset basis.
const FNV1A_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64-bit prime.
const FNV1A_PRIME: u64 = 0x0100_0000_01b3;

/// Compute an FNV-1a 64-bit hash of a byte slice.
#[must_use]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV1A_OFFSET_BASIS;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV1A_PRIME);
    }
    hash
}

/// Feed a UTF-8 string into an in-progress FNV-1a hash.
pub fn fnv1a_feed(hash: &mut u64, s: &str) {
    for &b in s.as_bytes() {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(FNV1A_PRIME);
    }
}

/// Feed a `u64` as 8 little-endian bytes into an in-progress FNV-1a hash.
pub fn fnv1a_feed_u64(hash: &mut u64, v: u64) {
    for b in v.to_le_bytes() {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(FNV1A_PRIME);
    }
}

/// Feed a role into an in-progress FNV-1a hash.
pub fn fnv1a_feed_role(hash: &mut u64, role: Role) {
    fnv1a_feed(hash, role.short_name());
}

/// Feed an optional string into an in-progress FNV-1a hash.
/// Does NOT prefix with a length — the caller must handle framing for
/// optional fields (see `element_signature`).
pub fn fnv1a_feed_opt(hash: &mut u64, s: Option<&str>) {
    if let Some(s) = s {
        fnv1a_feed(hash, s);
    }
}

/// Feed an `i64` as 8 little-endian bytes into an in-progress FNV-1a hash.
pub fn fnv1a_feed_i64(hash: &mut u64, v: i64) {
    for b in v.to_le_bytes() {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(FNV1A_PRIME);
    }
}

/// Feed the identity fields (`role`, `name`, `identifier`, `native_role`) into an
/// in-progress FNV-1a hash. Used by both `element_signature` and
/// `element_signature_with_bounds`.
fn feed_identity_fields(
    h: &mut u64,
    role: Role,
    name: Option<&str>,
    identifier: Option<&str>,
    native_role: Option<&str>,
) {
    fnv1a_feed_u64(h, role.short_name().len() as u64);
    fnv1a_feed_role(h, role);

    fnv1a_feed_u64(h, name.map_or(0, |s| s.len() as u64));
    fnv1a_feed_opt(h, name);

    fnv1a_feed_u64(h, identifier.map_or(0, |s| s.len() as u64));
    fnv1a_feed_opt(h, identifier);

    fnv1a_feed_u64(h, native_role.map_or(0, |s| s.len() as u64));
    fnv1a_feed_opt(h, native_role);
}

/// Compute a content-based signature for element identity fields.
///
/// Uses length-prefixed FNV-1a 64-bit hashing to prevent field boundary
/// ambiguity even if fields contain `\0` bytes. Each field is hashed as:
///   `length_as_u64_le8` || `field_bytes`
///
/// Optional fields that are `None` contribute length=0 (no field bytes).
///
/// Fields hashed (in order): `role`, `name`, `identifier`, `native_role`.
///
/// Same content → same signature across snapshots.
/// Changed content → different signature (the element identity changed).
/// Undifferentiated elements (same role, unnamed, un-identified,
/// same `native_role`) → same signature.
#[must_use]
pub fn element_signature(
    role: Role,
    name: Option<&str>,
    identifier: Option<&str>,
    native_role: Option<&str>,
) -> u64 {
    let mut h = FNV1A_OFFSET_BASIS;
    feed_identity_fields(&mut h, role, name, identifier, native_role);
    h
}

/// Compute a content + bounds signature for element identity.
///
/// Same as [`element_signature`] but folds rounded element bounds into the
/// hash. Disambiguates content-identical elements at different positions.
/// Each coordinate is rounded to the nearest integer to tolerate f64
/// floating-point noise.
///
/// Changes when the element moves OR its content changes — stricter than
/// `element_signature`, useful as a tiebreaker for undifferentiated elements.
#[must_use]
pub fn element_signature_with_bounds(
    role: Role,
    name: Option<&str>,
    identifier: Option<&str>,
    native_role: Option<&str>,
    bounds: Option<crate::core::types::Rect>,
) -> u64 {
    let mut h = FNV1A_OFFSET_BASIS;
    feed_identity_fields(&mut h, role, name, identifier, native_role);

    // Bounds: 4 length-prefixed f64 values rounded to nearest integer.
    // Rounding tolerates f64 floating-point noise while still distinguishing
    // elements at meaningfully different positions.
    if let Some(b) = bounds {
        fnv1a_feed_u64(&mut h, 1); // marker — bounds present
        #[expect(
            clippy::cast_possible_truncation,
            reason = "rounded f64 values (<10^9) safely fit in i64"
        )]
        {
            fnv1a_feed_i64(&mut h, b.x.round() as i64);
            fnv1a_feed_i64(&mut h, b.y.round() as i64);
            fnv1a_feed_i64(&mut h, b.width.round() as i64);
            fnv1a_feed_i64(&mut h, b.height.round() as i64);
        }
    } else {
        fnv1a_feed_u64(&mut h, 0); // marker — bounds absent
    }

    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::role::Role;

    // -----------------------------------------------------------------------
    // Known-answer test vectors from the `fnv` crate (rust-fnv, Apache-2.0 /
    // MIT). These validate correctness against the reference implementation.
    // See https://github.com/servo/rust-fnv for the full set.
    // -----------------------------------------------------------------------

    fn fnv1a_test(bytes: &[u8]) -> u64 {
        fnv1a_64(bytes)
    }

    #[test]
    fn known_answer_empty() {
        assert_eq!(fnv1a_test(b""), 0xcbf29ce484222325);
    }

    #[test]
    fn known_answer_single_byte() {
        assert_eq!(fnv1a_test(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a_test(b"b"), 0xaf63df4c8601f1a5);
        assert_eq!(fnv1a_test(b"c"), 0xaf63de4c8601eff2);
    }

    #[test]
    fn known_answer_short_strings() {
        assert_eq!(fnv1a_test(b"foo"), 0xdcb27518fed9d577);
        assert_eq!(fnv1a_test(b"foobar"), 0x85944171f73967e8);
        assert_eq!(fnv1a_test(b"hello"), 0xa430d84680aabd0b);
    }

    #[test]
    fn known_answer_with_nulls() {
        assert_eq!(fnv1a_test(b"\0"), 0xaf63bd4c8601b7df);
        assert_eq!(fnv1a_test(b"foo\0"), 0xdd1270790c25b935);
        assert_eq!(fnv1a_test(b"foobar\0"), 0x34531ca7168b8f38);
    }

    #[test]
    fn known_answer_chongo() {
        assert_eq!(
            fnv1a_test(b"chongo <Landon Curt Noll> /\\../\\"),
            0x2c8f4c9af81bcf06
        );
        assert_eq!(
            fnv1a_test(b"http://en.wikipedia.org/wiki/Fowler_Noll_Vo_hash"),
            0xd9b957fb7fe794c5
        );
    }

    #[test]
    fn known_answer_overflow_adjacent() {
        // Byte sequences that exercise carry propagation in wrapping_mul
        assert_eq!(fnv1a_test(b"\xff\x00\x00\x01"), 0x6961196491cc682d);
        assert_eq!(fnv1a_test(b"\x01\x00\x00\xff"), 0xad2bb1774799dfe9);
        assert_eq!(fnv1a_test(b"\xff\x00\x00\x02"), 0x6961166491cc6314);
        assert_eq!(fnv1a_test(b"\x02\x00\x00\xff"), 0x8d1bb3904a3b1236);
    }

    #[test]
    fn known_answer_ip_addresses() {
        assert_eq!(fnv1a_test(b"127.0.0.1"), 0xaabafe7104d914be);
        assert_eq!(fnv1a_test(b"127.0.0.2"), 0xaabafd7104d9130b);
    }

    #[test]
    fn known_answer_repeated_bytes() {
        let bytes: Vec<u8> = (0..10).flat_map(|_| b"21701".iter().copied()).collect();
        assert_eq!(fnv1a_test(&bytes), 0xc4112ffb337a82fb);
    }

    // -----------------------------------------------------------------------
    // Our own correctness tests (structure, not known-answer vectors)
    // -----------------------------------------------------------------------

    #[test]
    fn fnv1a_deterministic() {
        let a = fnv1a_64(b"test data");
        let b = fnv1a_64(b"test data");
        assert_eq!(a, b);
    }

    #[test]
    fn fnv1a_different_inputs_differ() {
        let a = fnv1a_64(b"hello");
        let b = fnv1a_64(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn element_signature_deterministic() {
        let a = element_signature(Role::Button, Some("OK"), None, None);
        let b = element_signature(Role::Button, Some("OK"), None, None);
        assert_eq!(a, b);
    }

    #[test]
    fn element_signature_differs_by_role() {
        let btn = element_signature(Role::Button, Some("OK"), None, None);
        let txt = element_signature(Role::TextField, Some("OK"), None, None);
        assert_ne!(btn, txt);
    }

    #[test]
    fn element_signature_differs_by_name() {
        let ok = element_signature(Role::Button, Some("OK"), None, None);
        let cancel = element_signature(Role::Button, Some("Cancel"), None, None);
        assert_ne!(ok, cancel);
    }

    #[test]
    fn element_signature_without_name() {
        let unnamed = element_signature(Role::Button, None, None, None);
        let named = element_signature(Role::Button, Some("OK"), None, None);
        assert_ne!(unnamed, named);
    }

    #[test]
    fn element_signature_with_identifier() {
        let no_id = element_signature(Role::Button, Some("OK"), None, None);
        let with_id = element_signature(Role::Button, Some("OK"), Some("submit-btn"), None);
        assert_ne!(no_id, with_id);
    }

    #[test]
    fn element_signature_with_native_role() {
        let no_nr = element_signature(Role::Unknown, None, None, None);
        let with_nr = element_signature(Role::Unknown, None, None, Some("AXCustomElement"));
        assert_ne!(no_nr, with_nr);
    }

    #[test]
    fn element_signature_undifferentiated_elements_match() {
        // Two unnamed, un-identified buttons with the same role
        // should have the same signature (they're structurally identical)
        let a = element_signature(Role::Button, None, None, None);
        let b = element_signature(Role::Button, None, None, None);
        assert_eq!(a, b);
    }

    #[test]
    fn element_signature_zero_is_unlikely() {
        // The FNV-1a offset basis is non-zero and role is always present,
        // so signature should never be zero for a real element.
        let sig = element_signature(Role::Button, None, None, None);
        assert_ne!(sig, 0);
    }

    #[test]
    fn fnv1a_feed_sequential_combining() {
        // Test that feeding "hello" then "world" is equivalent to
        // calling fnv1a_64 on the concatenated bytes
        let combined = fnv1a_64(b"helloworld");

        let mut h = FNV1A_OFFSET_BASIS;
        fnv1a_feed(&mut h, "hello");
        fnv1a_feed(&mut h, "world");
        assert_eq!(h, combined);
    }

    #[test]
    fn fnv1a_length_prefix_prevents_ambiguity() {
        // "AB" + "" should hash differently from "A" + "B" when length-prefixed
        let mut a = FNV1A_OFFSET_BASIS;
        fnv1a_feed_u64(&mut a, 2);
        fnv1a_feed(&mut a, "AB");
        fnv1a_feed_u64(&mut a, 0);

        let mut b = FNV1A_OFFSET_BASIS;
        fnv1a_feed_u64(&mut b, 1);
        fnv1a_feed(&mut b, "A");
        fnv1a_feed_u64(&mut b, 1);
        fnv1a_feed(&mut b, "B");

        assert_ne!(a, b);
    }

    #[test]
    fn fnv1a_feed_u64_deterministic() {
        let mut a = FNV1A_OFFSET_BASIS;
        fnv1a_feed_u64(&mut a, 42);
        let mut b = FNV1A_OFFSET_BASIS;
        fnv1a_feed_u64(&mut b, 42);
        assert_eq!(a, b);
    }
}
