//! macOS DarwinProvider backend.
//!
//! Implements the `DesktopProvider` trait using:
//! - AXUIElement (ApplicationServices) for accessibility tree walking
//! - CGEvent (CoreGraphics) for mouse/keyboard input simulation
//! - CGWindowList (CoreGraphics) for window enumeration
//! - NSWorkspace (AppKit) for application listing
//! - Vision framework for OCR
//! - `screencapture` CLI for screenshots

#[expect(
    dead_code,
    reason = "FFI bindings used from other modules after cfg gate"
)]
mod ffi;

pub mod annotation;
pub mod app;
pub mod cf_convert;
pub mod hit_test;
pub mod input;
pub mod key_code;
pub mod ocr;
pub mod role;
pub mod saliency;
pub mod screenshot;
pub mod snapshot;
pub mod text_attrs;

use std::sync::Mutex;

use crate::core::errors::ForepawError;
use crate::platform::{AppTarget, DesktopProvider, WindowTarget};

/// macOS implementation of `DesktopProvider`.
#[derive(Debug)]
pub struct DarwinProvider {
    /// Ref→`AXUIElement` cache from the most recent `snapshot`, used to resolve
    /// refs in O(1) without re-walking the AX tree. Replaced wholesale on each
    /// `snapshot`; handles are retained on insertion and released on eviction.
    /// Bounded by the interactive-node count of the single latest snapshot —
    /// repeated snapshots replace (don't accumulate), so there is no unbounded
    /// growth, only retain/release churn proportional to snapshot size.
    /// Darwin-internal — never exposed across the public API.
    ref_handles: Mutex<snapshot::RefHandleMap>,
    /// uid→handle cache, parallel to `ref_handles` but covering every node
    /// (so uid-keyed text-attr queries reach StaticText/Heading).
    uid_handles: Mutex<snapshot::UidHandleMap>,
}

impl DarwinProvider {
    /// Create a new Darwin platform provider.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ref_handles: Mutex::new(snapshot::RefHandleMap::empty()),
            uid_handles: Mutex::new(snapshot::UidHandleMap::empty()),
        }
    }

    /// Look up a retained handle for `ref_id` from the last snapshot's cache.
    ///
    /// Returns `None` if not cached. The returned handle carries a +1 retain so
    /// it stays valid after the lock is released; callers use it without
    /// releasing (matching the historical resolve contract).
    fn cached_handle(&self, ref_id: i32) -> Option<ffi::AXUIElementRef> {
        let cache = self.ref_handles.lock().expect("ref_handles mutex poisoned");
        cache.get(ref_id).map(|handle| {
            // SAFETY: CFRetain under the lock so the handle outlives the scope.
            // Balanced by the cache's own retain (released on snapshot replace).
            unsafe { ffi::CFRetain(handle.0 as ffi::CFTypeRef) };
            *handle
        })
    }

    /// Look up a retained handle for `uid` from the last snapshot's cache.
    fn cached_uid_handle(&self, uid: u64) -> Option<ffi::AXUIElementRef> {
        let cache = self.uid_handles.lock().expect("uid_handles mutex poisoned");
        cache.get(uid).map(|handle| {
            // SAFETY: CFRetain under the lock so the handle outlives the scope.
            unsafe { ffi::CFRetain(handle.0 as ffi::CFTypeRef) };
            *handle
        })
    }
}

impl Default for DarwinProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopProvider for DarwinProvider {
    fn list_apps(&self) -> Result<Vec<crate::platform::AppInfo>, ForepawError> {
        app::list_apps()
    }

    fn list_windows(
        &self,
        app: Option<&AppTarget>,
    ) -> Result<Vec<crate::platform::WindowInfo>, ForepawError> {
        app::list_windows(app)
    }

    fn snapshot(
        &self,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        options: &crate::platform::SnapshotOptions,
    ) -> Result<crate::core::element_tree::ElementTree, ForepawError> {
        let (tree, handles, uid_handles) = snapshot::snapshot(app, window, options)?;
        // Replace the ref→handle cache. The old map drops and releases its
        // handles; the new one is retained from this snapshot's walk.
        *self.ref_handles.lock().expect("ref_handles mutex poisoned") = handles;
        *self.uid_handles.lock().expect("uid_handles mutex poisoned") = uid_handles;
        Ok(tree)
    }

    fn screenshot(
        &self,
        params: &crate::platform::ScreenshotParams,
    ) -> Result<crate::platform::ScreenshotResult, ForepawError> {
        screenshot::screenshot(params)
    }

    fn ocr(
        &self,
        app: Option<&AppTarget>,
        window: Option<&WindowTarget>,
        find: Option<&str>,
        screenshot_options: Option<&crate::core::encoder_detection::ScreenshotOptions>,
    ) -> Result<crate::core::ocr_result::OCROutput, ForepawError> {
        ocr::ocr(app, window, find, screenshot_options)
    }

    fn click_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        app: &AppTarget,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        let cached = self.cached_handle(reference.id);
        input::click_ref(reference, app, options, cached)
    }

    fn click_at_point(
        &self,
        point: crate::core::types::Point,
        app: &AppTarget,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::click_at_point(point, app, options)
    }

    fn click_region(
        &self,
        region: crate::core::types::Rect,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::click_region(region, app, window, options)
    }

    fn hover_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        app: &AppTarget,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        let cached = self.cached_handle(reference.id);
        input::hover_ref(reference, app, cached)
    }

    fn hover_at_point(
        &self,
        point: crate::core::types::Point,
        app: Option<&AppTarget>,
        smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::hover_at_point(point, app, smooth)
    }

    fn hover_region(
        &self,
        region: crate::core::types::Rect,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::hover_region(region, app, window, smooth)
    }

    fn ocr_hover(
        &self,
        text: &str,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        ocr::ocr_hover(text, app, window, index)
    }

    fn type_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        text: &str,
        app: &AppTarget,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        let cached = self.cached_handle(reference.id);
        input::type_ref(reference, text, app, cached)
    }

    fn keyboard_type(
        &self,
        text: &str,
        app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::keyboard_type(text, app)
    }

    fn press(
        &self,
        keys: &crate::core::key_combo::KeyCombo,
        app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::press_key(keys, app)
    }

    fn scroll(
        &self,
        direction: &str,
        amount: u32,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        reference: Option<crate::core::element_tree::ElementRef>,
        at: Option<crate::core::types::Point>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        let cached = reference.and_then(|r| self.cached_handle(r.id));
        input::scroll(direction, amount, app, window, reference, at, cached)
    }

    fn drag_path(
        &self,
        path: &[crate::core::types::Point],
        options: &crate::core::key_combo::DragOptions,
        app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::drag_path(path, options, app)
    }

    fn drag_refs(
        &self,
        from: crate::core::element_tree::ElementRef,
        to: crate::core::element_tree::ElementRef,
        app: &AppTarget,
        options: &crate::core::key_combo::DragOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        let from_cached = self.cached_handle(from.id);
        let to_cached = self.cached_handle(to.id);
        input::drag_refs(from, to, app, options, from_cached, to_cached)
    }

    fn ocr_click(
        &self,
        text: &str,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        options: &crate::core::key_combo::ClickOptions,
        index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        ocr::ocr_click(text, app, window, options, index)
    }

    fn wait(
        &self,
        text: &str,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        timeout: f64,
        interval: f64,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        ocr::wait(text, app, window, timeout, interval)
    }

    fn resolve_ref_position(
        &self,
        reference: crate::core::element_tree::ElementRef,
        app: &AppTarget,
    ) -> Result<crate::core::types::Point, ForepawError> {
        let cached = self.cached_handle(reference.id);
        snapshot::resolve_ref_position(reference.id, app, cached)
    }

    fn resolve_ref_bounds(
        &self,
        reference: crate::core::element_tree::ElementRef,
        app: &AppTarget,
    ) -> Result<crate::core::types::Rect, ForepawError> {
        let cached = self.cached_handle(reference.id);
        snapshot::resolve_ref_bounds(reference.id, app, cached)
    }

    fn element_at_point(
        &self,
        point: crate::core::types::Point,
        app_hint: Option<&AppTarget>,
    ) -> Result<crate::platform::HitTestResult, ForepawError> {
        hit_test::element_at_point(point, app_hint)
    }

    fn has_permissions(&self) -> bool {
        // SAFETY: AXIsProcessTrusted is a read-only system call with no side effects.
        unsafe { ffi::AXIsProcessTrusted() != 0 }
    }

    fn has_screen_recording_permission(&self) -> bool {
        // TCC permissions are inherited by child processes (e.g. running
        // forepaw from Ghostty inherits Ghostty's Screen Recording grant).
        // SAFETY: CGPreflightScreenCaptureAccess is a read-only system call.
        unsafe { ffi::CGPreflightScreenCaptureAccess() != 0 }
    }

    fn validate_screen_recording(&self) -> bool {
        // CGPreflightScreenCaptureAccess can return true while window data is
        // still redacted (new binary, macOS cache, etc). The only reliable
        // check is to query the window list and see if regular apps show up.
        // System apps (System Settings, Finder) may appear even without SR,
        // so we check for third-party apps specifically.
        let Ok(apps) = app::list_apps() else {
            return false;
        };
        // Filter to third-party apps (skip Apple bundles starting with com.apple)
        let third_party: Vec<&str> = apps
            .iter()
            .filter(|a| {
                !a.bundle_id
                    .as_ref()
                    .is_some_and(|b| b.starts_with("com.apple."))
            })
            .map(|a| a.name.as_str())
            .collect();
        if third_party.is_empty() {
            return true; // no third-party apps running, can't validate
        }

        // Get raw window list
        // SAFETY: CGWindowListCopyWindowInfo returns a CFArray the caller owns.
        // No pointer dereferences of caller-controlled data.
        let window_list = unsafe {
            ffi::CGWindowListCopyWindowInfo(
                ffi::CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY,
                ffi::K_CG_NULL_WINDOW_ID,
            )
        };
        if window_list.is_null() {
            return false;
        }

        // SAFETY: window_list is a valid CFArray from CGWindowListCopyWindowInfo.
        let count = unsafe { ffi::CFArrayGetCount(window_list) };
        let mut found_app_window = false;

        'outer: for i in 0..count {
            let info =
                // SAFETY: index is in bounds (0..count).
                unsafe { ffi::CFArrayGetValueAtIndex(window_list, i as _) as ffi::CFDictionaryRef };
            if info.is_null() {
                continue;
            }
            // SAFETY: info is a valid CFDictionary from the window list.
            if let Some(owner) =
                // SAFETY: get_dict_string_ref reads from a valid CFDictionary.
                unsafe { cf_convert::get_dict_string_ref(info, ffi::kCGWindowOwnerName) }
            {
                if third_party.iter().any(|name| *name == owner) {
                    found_app_window = true;
                    break 'outer;
                }
            }
        }

        // SAFETY: window_list is a valid CFType that we own (from CopyWindowInfo).
        unsafe { ffi::CFRelease(window_list as ffi::CFTypeRef) };

        found_app_window
    }

    fn request_permissions(&self) -> bool {
        // Build CFDictionary {"AXTrustedCheckOptionPrompt": true}
        // to trigger the system accessibility permission dialog.
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "CF create + query + 2 releases"
        )]
        // SAFETY: CFDictionaryCreate and AXIsProcessTrustedWithOptions are
        // system calls. prompt_key is a CFString we own. All CF objects are
        // released before returning.
        unsafe {
            let prompt_key = cf_convert::cf_string_from_str("AXTrustedCheckOptionPrompt");
            let prompt_val = ffi::kCFBooleanTrue;

            let keys = [prompt_key.cast::<std::ffi::c_void>()];
            let values = [prompt_val.cast::<std::ffi::c_void>()];

            let dict = ffi::CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &raw const ffi::kCFTypeDictionaryKeyCallBacks,
                &raw const ffi::kCFTypeDictionaryValueCallBacks,
            );
            let result = ffi::AXIsProcessTrustedWithOptions(dict) != 0;
            ffi::CFRelease(dict as ffi::CFTypeRef);
            ffi::CFRelease(prompt_key as ffi::CFTypeRef);
            result
        }
    }

    fn request_screen_recording_permission(&self) -> bool {
        // SAFETY: CGRequestScreenCaptureAccess is a system call that prompts the user.
        unsafe { ffi::CGRequestScreenCaptureAccess() != 0 }
    }

    fn get_text_attributes(
        &self,
        app: &AppTarget,
        reference: crate::core::element_tree::ElementRef,
    ) -> Result<Option<crate::core::text_attrs::TextAttrsResult>, ForepawError> {
        // Resolve ref to AX element, then extract text attributes.
        let cached = self.cached_handle(reference.id);
        let element = snapshot::resolve_ref_element(reference.id, app, cached)?;
        // SAFETY: element is a valid AXUIElementRef from resolve_ref_element.
        let attrs = unsafe { text_attrs::get_text_attributes(element) };
        Ok(attrs)
    }

    fn get_text_attributes_by_uid(
        &self,
        uid: u64,
    ) -> Result<Option<crate::core::text_attrs::TextAttrsResult>, ForepawError> {
        // Resolve uid to a retained AX handle from the last snapshot's cache,
        // then extract text attributes. Unlike `get_text_attributes`, there is
        // no re-walk fallback: a uid without a cached handle (e.g. before any
        // snapshot) returns `Ok(None)`.
        let Some(handle) = self.cached_uid_handle(uid) else {
            return Ok(None);
        };
        // SAFETY: handle is a valid AXUIElementRef retained from the snapshot.
        let attrs = unsafe { text_attrs::get_text_attributes(handle) };
        Ok(attrs)
    }
}
