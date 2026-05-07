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

pub mod app;
pub mod key_code;
pub mod snapshot;

// TODO: implement remaining modules
// mod input;
// mod screenshot;
// mod ocr;
// mod annotation;
// mod saliency;

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

    fn list_windows(&self, app: Option<&str>) -> Result<Vec<crate::platform::WindowInfo>, ForepawError> {
        app::list_windows(app)
    }

    // TODO: implement remaining trait methods

    fn snapshot(
        &self,
        app: &str,
        options: &crate::platform::SnapshotOptions,
    ) -> Result<crate::core::element_tree::ElementTree, ForepawError> {
        snapshot::snapshot(app, options)
    }

    fn screenshot(
        &self,
        _params: &crate::platform::ScreenshotParams,
    ) -> Result<crate::platform::ScreenshotResult, ForepawError> {
        Err(ForepawError::ActionFailed("screenshot not yet implemented".into()))
    }

    fn ocr(
        &self,
        _app: Option<&str>,
        _window: Option<&str>,
        _find: Option<&str>,
        _screenshot_options: Option<&crate::platform::ScreenshotOptions>,
    ) -> Result<crate::core::ocr_result::OCROutput, ForepawError> {
        Err(ForepawError::ActionFailed("ocr not yet implemented".into()))
    }

    fn click_ref(
        &self,
        _ref: crate::core::element_tree::ElementRef,
        _app: &str,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("click not yet implemented".into()))
    }

    fn click_at_point(
        &self,
        _point: crate::core::types::Point,
        _app: &str,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("click not yet implemented".into()))
    }

    fn click_region(
        &self,
        _region: crate::core::types::Rect,
        _app: &str,
        _window: Option<&str>,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("click not yet implemented".into()))
    }

    fn hover_ref(
        &self,
        _ref: crate::core::element_tree::ElementRef,
        _app: &str,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("hover not yet implemented".into()))
    }

    fn hover_at_point(
        &self,
        _point: crate::core::types::Point,
        _app: Option<&str>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("hover not yet implemented".into()))
    }

    fn hover_region(
        &self,
        _region: crate::core::types::Rect,
        _app: &str,
        _window: Option<&str>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("hover not yet implemented".into()))
    }

    fn ocr_hover(
        &self,
        _text: &str,
        _app: &str,
        _window: Option<&str>,
        _index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("ocr-hover not yet implemented".into()))
    }

    fn type_ref(
        &self,
        _ref: crate::core::element_tree::ElementRef,
        _text: &str,
        _app: &str,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("type not yet implemented".into()))
    }

    fn keyboard_type(
        &self,
        _text: &str,
        _app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("keyboard-type not yet implemented".into()))
    }

    fn press(
        &self,
        _keys: &crate::core::key_combo::KeyCombo,
        _app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("press not yet implemented".into()))
    }

    fn scroll(
        &self,
        _direction: &str,
        _amount: u32,
        _app: &str,
        _window: Option<&str>,
        _ref: Option<crate::core::element_tree::ElementRef>,
        _at: Option<crate::core::types::Point>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("scroll not yet implemented".into()))
    }

    fn drag_path(
        &self,
        _path: &[crate::core::types::Point],
        _options: &crate::core::key_combo::DragOptions,
        _app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("drag not yet implemented".into()))
    }

    fn drag_refs(
        &self,
        _from: crate::core::element_tree::ElementRef,
        _to: crate::core::element_tree::ElementRef,
        _app: &str,
        _options: &crate::core::key_combo::DragOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("drag not yet implemented".into()))
    }

    fn ocr_click(
        &self,
        _text: &str,
        _app: &str,
        _window: Option<&str>,
        _options: &crate::core::key_combo::ClickOptions,
        _index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("ocr-click not yet implemented".into()))
    }

    fn wait(
        &self,
        _text: &str,
        _app: &str,
        _window: Option<&str>,
        _timeout: f64,
        _interval: f64,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed("wait not yet implemented".into()))
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
        let apps = match app::list_apps() {
            Ok(a) => a,
            Err(_) => return false,
        };
        // Filter to third-party apps (skip Apple bundles starting with com.apple)
        let third_party: Vec<&str> = apps
            .iter()
            .filter(|a| !a.bundle_id.as_ref().is_some_and(|b| b.starts_with("com.apple.")))
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
