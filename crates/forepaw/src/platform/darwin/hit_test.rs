//! macOS hit test: element at point via `AXUIElementCopyElementAtPosition`.
//!
//! Uses `AXUIElementCreateSystemWide()` for system-wide hit testing (no `--app`)
//! or `AXUIElementCreateApplication(pid)` for app-scoped (`--app AppName`).
//! Returns the deepest accessible element at the point plus parent chain.
//!
//! Coordinate system: screen coordinates (top-left relative), matching the
//! `Float32` x/y parameters of `AXUIElementCopyElementAtPosition`.

use crate::core::errors::ForepawError;
use crate::core::role::Role;
use crate::core::types::{Point, Rect};
use crate::platform::{AncestorInfo, AppTarget, HitTestResult};

use super::app::find_app_by_target;
use super::cf_convert::{cf_string_from_str, cf_string_to_rust};
use super::ffi::{
    kCFNull, AXError, AXUIElementCopyActionNames, AXUIElementCopyAttributeValue,
    AXUIElementCopyElementAtPosition, AXUIElementCreateApplication, AXUIElementCreateSystemWide,
    AXUIElementGetPid, AXUIElementRef, CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef,
    CFGetTypeID, CFRelease, CFStringGetTypeID, CFStringRef, CFTypeRef,
};
use super::snapshot::{
    fetch_batch_attributes, get_ax_string_attr, get_element_position, get_element_size, non_empty,
    Attr,
};

/// Performs a hit test at the given screen coordinates.
///
/// Returns the deepest accessibility element at the point and its ancestor chain.
/// When `app_hint` is `None`, searches all applications (system-wide via
/// `AXUIElementCreateSystemWide`). When scoped to an app name, searches only
/// within that application.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if no element is found at the position.
pub fn element_at_point(
    point: Point,
    app_hint: Option<&AppTarget>,
) -> Result<HitTestResult, ForepawError> {
    // 1. Create the scope element: system-wide for cross-app, or per-app
    let scope_element = if let Some(app) = app_hint {
        let running_app = find_app_by_target(app)?;
        // SAFETY: AXUIElementCreateApplication is a system call, no preconditions.
        unsafe { AXUIElementCreateApplication(running_app.processIdentifier()) }
    } else {
        // SAFETY: AXUIElementCreateSystemWide is a system call, no preconditions.
        unsafe { AXUIElementCreateSystemWide() }
    };

    // 2. Perform the hit test
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates fit in f32 without meaningful loss"
    )]
    let x = point.x as f32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates fit in f32 without meaningful loss"
    )]
    let y = point.y as f32;

    let mut hit_element = AXUIElementRef(std::ptr::null());
    // SAFETY: AXUIElementCopyElementAtPosition takes a screen coordinate and returns
    // a retained AXUIElementRef on success. scope_element is a valid AXUIElementRef.
    let err =
        unsafe { AXUIElementCopyElementAtPosition(scope_element, x, y, &raw mut hit_element) };

    if err != AXError::Success || hit_element.0.is_null() {
        return Err(ForepawError::ActionFailed("no element at position".into()));
    }

    // 3. Get the PID of the owning process
    let mut pid: i32 = 0;
    // SAFETY: AXUIElementGetPid reads the PID from the hit element.
    unsafe { AXUIElementGetPid(hit_element, &raw mut pid) };

    // 4. Batch-fetch attributes for the hit element
    let attrs = fetch_batch_attributes(hit_element);

    let role = attrs
        .as_ref()
        .and_then(|a| a.string(Attr::Role))
        .map_or(Role::Unknown, |s| super::role::ax_role_to_role(&s));
    let name = attrs
        .as_ref()
        .and_then(|a| non_empty(a.string(Attr::Title).as_ref()))
        .or_else(|| {
            attrs
                .as_ref()
                .and_then(|a| non_empty(a.string(Attr::Description).as_ref()))
        });
    let value = attrs.as_ref().and_then(|a| a.value_string(Attr::Value));
    let bounds = attrs
        .as_ref()
        .and_then(|a| a.bounds(Attr::Position, Attr::Size));

    // 5. Fetch available actions
    let actions = get_action_names(hit_element);

    // 6. Walk the parent (AXParent) chain: hit element → ... → window → application
    let mut ancestors: Vec<AncestorInfo> = Vec::new();
    let mut current = hit_element;
    while let Some(parent_element) = get_ax_parent_element(current) {
        let parent_role_str = get_ax_string_attr(parent_element, "AXRole").unwrap_or_default();
        let parent_role = super::role::ax_role_to_role(&parent_role_str);
        let parent_name = get_ax_string_attr(parent_element, "AXTitle").filter(|s| !s.is_empty());
        let parent_bounds = get_element_position(parent_element).and_then(|pos| {
            get_element_size(parent_element).map(|(w, h)| Rect::new(pos.x, pos.y, w, h))
        });

        ancestors.push(AncestorInfo {
            role: parent_role,
            name: parent_name,
            bounds: parent_bounds,
        });

        // Stop at the application root (no meaningful ancestors above it)
        if parent_role == Role::Application {
            break;
        }
        current = parent_element;
    }
    ancestors.reverse(); // root-first

    Ok(HitTestResult {
        role,
        name,
        value,
        bounds,
        actions,
        ancestors,
        pid,
    })
}

/// Fetch the action names for an `AXUIElement` via `AXUIElementCopyActionNames`.
fn get_action_names(element: AXUIElementRef) -> Vec<String> {
    let mut action_names: CFArrayRef = std::ptr::null();
    // SAFETY: AXUIElementCopyActionNames returns a CFArray of CFString action names.
    let err = unsafe { AXUIElementCopyActionNames(element, &raw mut action_names) };
    if err != AXError::Success || action_names.is_null() {
        return Vec::new();
    }

    let mut result = Vec::new();
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "CFArray iteration + CFString conversion + CFRelease all touch FFI"
    )]
    // SAFETY: action_names is a valid non-null CFArray. The remaining operations
    // (count, get value, convert string) are standard CFArray iteration.
    unsafe {
        let count = CFArrayGetCount(action_names);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "CFArray count fits in usize"
        )]
        #[expect(clippy::cast_sign_loss, reason = "CFArray count is non-negative")]
        let count_usize = count as usize;
        result.reserve(count_usize);
        for i in 0..count {
            let val = CFArrayGetValueAtIndex(action_names, i);
            if val.is_null() || val as CFTypeRef == kCFNull {
                continue;
            }
            if CFGetTypeID(val as CFTypeRef) == CFStringGetTypeID() {
                if let Some(s) = cf_string_to_rust(val as CFStringRef) {
                    if !s.is_empty() {
                        result.push(s);
                    }
                }
            }
        }
        CFRelease(action_names as CFTypeRef);
    }
    result
}

/// Get the `AXParent` of an `AXUIElement`.
fn get_ax_parent_element(element: AXUIElementRef) -> Option<AXUIElementRef> {
    let attr_cf = cf_string_from_str("AXParent");
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: AXUIElementCopyAttributeValue with AXParent on a valid element.
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &raw mut value) };
    // SAFETY: attr_cf is a CFString we own.
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return None;
    }
    // SAFETY: The parent value is a retained AXUIElementRef. We wrap it and
    // return it; the caller borrows it temporarily for attribute reads.
    Some(unsafe { AXUIElementRef::from_raw(value.cast::<std::ffi::c_void>()) })
}
