//! macOS DarwinProvider backend.
//!
//! Implements the `DesktopProvider` trait using:
//! - AXUIElement (ApplicationServices) for accessibility tree walking
//! - CGEvent (CoreGraphics) for mouse/keyboard input simulation
//! - CGWindowList (CoreGraphics) for window enumeration
//! - NSWorkspace (AppKit) for application listing
//! - Vision framework for OCR
//! - `screencapture` CLI for screenshots

#[allow(dead_code)]
mod ffi;

pub mod annotation;
pub mod app;
pub mod input;
pub mod key_code;
pub mod ocr;
pub mod saliency;
pub mod screenshot;
pub mod snapshot;

use crate::core::errors::ForepawError;
use crate::platform::DesktopProvider;

/// macOS implementation of `DesktopProvider`.
pub struct DarwinProvider;

impl DarwinProvider {
    pub fn new() -> Self {
        Self
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
        app: Option<&str>,
    ) -> Result<Vec<crate::platform::WindowInfo>, ForepawError> {
        app::list_windows(app)
    }

    fn snapshot(
        &self,
        app: &str,
        options: &crate::platform::SnapshotOptions,
    ) -> Result<crate::core::element_tree::ElementTree, ForepawError> {
        snapshot::snapshot(app, options)
    }

    fn screenshot(
        &self,
        params: &crate::platform::ScreenshotParams,
    ) -> Result<crate::platform::ScreenshotResult, ForepawError> {
        screenshot::screenshot(params)
    }

    fn ocr(
        &self,
        app: Option<&str>,
        window: Option<&str>,
        find: Option<&str>,
        screenshot_options: Option<&crate::platform::ScreenshotOptions>,
    ) -> Result<crate::core::ocr_result::OCROutput, ForepawError> {
        ocr::ocr(app, window, find, screenshot_options)
    }

    fn click_ref(
        &self,
        r#ref: crate::core::element_tree::ElementRef,
        app: &str,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::click_ref(r#ref, app, options)
    }

    fn click_at_point(
        &self,
        point: crate::core::types::Point,
        app: &str,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::click_at_point(point, app, options)
    }

    fn click_region(
        &self,
        region: crate::core::types::Rect,
        app: &str,
        window: Option<&str>,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::click_region(region, app, window, options)
    }

    fn hover_ref(
        &self,
        r#ref: crate::core::element_tree::ElementRef,
        app: &str,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::hover_ref(r#ref, app)
    }

    fn hover_at_point(
        &self,
        point: crate::core::types::Point,
        app: Option<&str>,
        smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::hover_at_point(point, app, smooth)
    }

    fn hover_region(
        &self,
        region: crate::core::types::Rect,
        app: &str,
        window: Option<&str>,
        smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::hover_region(region, app, window, smooth)
    }

    fn ocr_hover(
        &self,
        text: &str,
        app: &str,
        window: Option<&str>,
        index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        ocr::ocr_hover(text, app, window, index)
    }

    fn type_ref(
        &self,
        r#ref: crate::core::element_tree::ElementRef,
        text: &str,
        app: &str,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::type_ref(r#ref, text, app)
    }

    fn keyboard_type(
        &self,
        text: &str,
        app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::keyboard_type(text, app)
    }

    fn press(
        &self,
        keys: &crate::core::key_combo::KeyCombo,
        app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::press_key(keys, app)
    }

    fn scroll(
        &self,
        direction: &str,
        amount: u32,
        app: &str,
        window: Option<&str>,
        r#ref: Option<crate::core::element_tree::ElementRef>,
        at: Option<crate::core::types::Point>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::scroll(direction, amount, app, window, r#ref, at)
    }

    fn drag_path(
        &self,
        path: &[crate::core::types::Point],
        options: &crate::core::key_combo::DragOptions,
        app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::drag_path(path, options, app)
    }

    fn drag_refs(
        &self,
        from: crate::core::element_tree::ElementRef,
        to: crate::core::element_tree::ElementRef,
        app: &str,
        options: &crate::core::key_combo::DragOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        input::drag_refs(from, to, app, options)
    }

    fn ocr_click(
        &self,
        text: &str,
        app: &str,
        window: Option<&str>,
        options: &crate::core::key_combo::ClickOptions,
        index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        ocr::ocr_click(text, app, window, options, index)
    }

    fn wait(
        &self,
        text: &str,
        app: &str,
        window: Option<&str>,
        timeout: f64,
        interval: f64,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        ocr::wait(text, app, window, timeout, interval)
    }

    fn resolve_ref_position(
        &self,
        r#ref: crate::core::element_tree::ElementRef,
        app: &str,
    ) -> Result<crate::core::types::Point, ForepawError> {
        snapshot::resolve_ref_position(r#ref.id, app)
    }

    fn resolve_ref_bounds(
        &self,
        r#ref: crate::core::element_tree::ElementRef,
        app: &str,
    ) -> Result<crate::core::types::Rect, ForepawError> {
        snapshot::resolve_ref_bounds(r#ref.id, app)
    }

    fn has_permissions(&self) -> bool {
        unsafe { ffi::AXIsProcessTrusted() != 0 }
    }

    fn has_screen_recording_permission(&self) -> bool {
        // TCC permissions are inherited by child processes (e.g. running
        // forepaw from Ghostty inherits Ghostty's Screen Recording grant).
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
        let window_list = unsafe {
            ffi::CGWindowListCopyWindowInfo(
                ffi::CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY,
                ffi::K_CG_NULL_WINDOW_ID,
            )
        };
        if window_list.is_null() {
            return false;
        }

        let count = unsafe { ffi::CFArrayGetCount(window_list) };
        let mut found_app_window = false;

        'outer: for i in 0..count {
            let info =
                unsafe { ffi::CFArrayGetValueAtIndex(window_list, i as _) as ffi::CFDictionaryRef };
            if info.is_null() {
                continue;
            }
            if let Some(owner) = unsafe { app::get_dict_string(info, ffi::kCGWindowOwnerName) } {
                if third_party.iter().any(|name| *name == owner) {
                    found_app_window = true;
                    break 'outer;
                }
            }
        }

        unsafe { ffi::CFRelease(window_list as ffi::CFTypeRef) };

        found_app_window
    }

    fn request_permissions(&self) -> bool {
        // Build CFDictionary {"AXTrustedCheckOptionPrompt": true}
        // to trigger the system accessibility permission dialog.
        unsafe {
            let prompt_key = app::cf_string_from_str("AXTrustedCheckOptionPrompt");
            let prompt_val = ffi::kCFBooleanTrue;

            let keys = [prompt_key as *const std::ffi::c_void];
            let values = [prompt_val as *const std::ffi::c_void];

            let dict = ffi::CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &ffi::kCFTypeDictionaryKeyCallBacks,
                &ffi::kCFTypeDictionaryValueCallBacks,
            );
            let result = ffi::AXIsProcessTrustedWithOptions(dict) != 0;
            ffi::CFRelease(dict as ffi::CFTypeRef);
            ffi::CFRelease(prompt_key as ffi::CFTypeRef);
            result
        }
    }

    fn request_screen_recording_permission(&self) -> bool {
        unsafe { ffi::CGRequestScreenCaptureAccess() != 0 }
    }
}
