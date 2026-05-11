//! Windows backend using Win32 APIs and UI Automation.
//!
//! Implements the `DesktopProvider` trait using:
//! - EnumWindows / GetWindowThreadProcessId for app and window enumeration
//! - IUIAutomation + ControlView TreeWalker for accessibility tree walking
//! - SendInput for keyboard/mouse input (future)
//! - Windows.Media.Ocr for OCR (future)

pub mod app;
pub mod snapshot;

use crate::core::errors::ForepawError;
use crate::platform::DesktopProvider;

/// Windows implementation of `DesktopProvider`.
pub struct WindowsProvider;

impl WindowsProvider {
    pub fn new() -> Self {
        snapshot::init_com();
        Self
    }
}

impl Default for WindowsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopProvider for WindowsProvider {
    fn list_apps(&self) -> Result<Vec<crate::platform::AppInfo>, ForepawError> {
        app::list_apps()
    }

    fn list_windows(
        &self,
        app: Option<&str>,
    ) -> Result<Vec<crate::platform::WindowInfo>, ForepawError> {
        app::list_windows(app)
    }

    // --- Observation (stubs for now) ---

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
        Err(ForepawError::ActionFailed(
            "screenshot not yet implemented on Windows".into(),
        ))
    }

    fn ocr(
        &self,
        _app: Option<&str>,
        _window: Option<&str>,
        _find: Option<&str>,
        _screenshot_options: Option<&crate::platform::ScreenshotOptions>,
    ) -> Result<crate::core::ocr_result::OCROutput, ForepawError> {
        Err(ForepawError::ActionFailed(
            "ocr not yet implemented on Windows".into(),
        ))
    }

    // --- Actions (stubs) ---

    fn click_ref(
        &self,
        r#ref: crate::core::element_tree::ElementRef,
        _app: &str,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "click not yet implemented on Windows (ref: {ref})"
        )))
    }

    fn click_at_point(
        &self,
        _point: crate::core::types::Point,
        _app: &str,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "click not yet implemented on Windows".into(),
        ))
    }

    fn click_region(
        &self,
        _region: crate::core::types::Rect,
        _app: &str,
        _window: Option<&str>,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "click_region not yet implemented on Windows".into(),
        ))
    }

    fn hover_ref(
        &self,
        r#ref: crate::core::element_tree::ElementRef,
        _app: &str,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "hover not yet implemented on Windows (ref: {ref})"
        )))
    }

    fn hover_at_point(
        &self,
        _point: crate::core::types::Point,
        _app: Option<&str>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "hover not yet implemented on Windows".into(),
        ))
    }

    fn hover_region(
        &self,
        _region: crate::core::types::Rect,
        _app: &str,
        _window: Option<&str>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "hover_region not yet implemented on Windows".into(),
        ))
    }

    fn ocr_hover(
        &self,
        _text: &str,
        _app: &str,
        _window: Option<&str>,
        _index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "ocr_hover not yet implemented on Windows".into(),
        ))
    }

    fn type_ref(
        &self,
        r#ref: crate::core::element_tree::ElementRef,
        _text: &str,
        _app: &str,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "type not yet implemented on Windows (ref: {ref})"
        )))
    }

    fn keyboard_type(
        &self,
        _text: &str,
        _app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "keyboard_type not yet implemented on Windows".into(),
        ))
    }

    fn press(
        &self,
        _keys: &crate::core::key_combo::KeyCombo,
        _app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "press not yet implemented on Windows".into(),
        ))
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
        Err(ForepawError::ActionFailed(
            "scroll not yet implemented on Windows".into(),
        ))
    }

    fn drag_path(
        &self,
        _path: &[crate::core::types::Point],
        _options: &crate::core::key_combo::DragOptions,
        _app: Option<&str>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "drag not yet implemented on Windows".into(),
        ))
    }

    fn drag_refs(
        &self,
        _from: crate::core::element_tree::ElementRef,
        _to: crate::core::element_tree::ElementRef,
        _app: &str,
        _options: &crate::core::key_combo::DragOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "drag not yet implemented on Windows".into(),
        ))
    }

    fn ocr_click(
        &self,
        _text: &str,
        _app: &str,
        _window: Option<&str>,
        _options: &crate::core::key_combo::ClickOptions,
        _index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "ocr_click not yet implemented on Windows".into(),
        ))
    }

    fn wait(
        &self,
        _text: &str,
        _app: &str,
        _window: Option<&str>,
        _timeout: f64,
        _interval: f64,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "wait not yet implemented on Windows".into(),
        ))
    }

    // --- Utility ---

    fn resolve_ref_position(
        &self,
        _ref: crate::core::element_tree::ElementRef,
        _app: &str,
    ) -> Result<crate::core::types::Point, ForepawError> {
        Err(ForepawError::ActionFailed(
            "resolve_ref_position not yet implemented on Windows".into(),
        ))
    }

    fn resolve_ref_bounds(
        &self,
        _ref: crate::core::element_tree::ElementRef,
        _app: &str,
    ) -> Result<crate::core::types::Rect, ForepawError> {
        Err(ForepawError::ActionFailed(
            "resolve_ref_bounds not yet implemented on Windows".into(),
        ))
    }

    // --- Permissions ---
    // Windows has no equivalent of macOS Accessibility / Screen Recording gates.
    // UIA works without special permissions. SendInput works for the calling desktop.

    fn has_permissions(&self) -> bool {
        true
    }

    fn has_screen_recording_permission(&self) -> bool {
        true
    }

    fn validate_screen_recording(&self) -> bool {
        true
    }

    fn request_permissions(&self) -> bool {
        true
    }

    fn request_screen_recording_permission(&self) -> bool {
        true
    }
}
