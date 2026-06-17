//! Windows backend using Win32 APIs and UI Automation.
//!
//! Implements the `DesktopProvider` trait using:
//! - `EnumWindows` / `GetWindowThreadProcessId` for app and window enumeration
//! - `IUIAutomation` + `ControlView` `TreeWalker` for accessibility tree walking
//! - GDI `BitBlt` for screenshots (physical pixels, DPI-aware)
//! - Windows.Media.Ocr (`WinRT`) for OCR
//! - `SendInput` for keyboard/mouse input (future)

pub mod app;
pub mod hit_test;
pub mod ocr;
pub mod role;
pub mod screenshot;
pub mod snapshot;

use crate::core::errors::ForepawError;
use crate::platform::{AppTarget, DesktopProvider, WindowTarget};

/// Windows implementation of `DesktopProvider`.
#[derive(Debug)]
pub struct WindowsProvider;

impl WindowsProvider {
    /// Create a new Windows platform provider.
    #[must_use]
    pub fn new() -> Self {
        screenshot::init_dpi_awareness();
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
        app: Option<&AppTarget>,
    ) -> Result<Vec<crate::platform::WindowInfo>, ForepawError> {
        app::list_windows(app)
    }

    // --- Observation (stubs for now) ---

    fn snapshot(
        &self,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        options: &crate::platform::SnapshotOptions,
    ) -> Result<crate::core::element_tree::ElementTree, ForepawError> {
        snapshot::snapshot(app, window, options)
    }

    fn screenshot(
        &self,
        params: &crate::platform::ScreenshotParams,
    ) -> Result<crate::platform::ScreenshotResult, ForepawError> {
        let path = screenshot::screenshot(params.app, params.window)?;
        Ok(crate::platform::ScreenshotResult {
            image: crate::platform::ScreenshotImage::Path(path),
            annotations: None,
            legend: None,
        })
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

    // --- Actions (stubs) ---

    fn click_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        _app: &AppTarget,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "click not yet implemented on Windows (ref: {reference})"
        )))
    }

    fn click_at_point(
        &self,
        _point: crate::core::types::Point,
        _app: &AppTarget,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "click not yet implemented on Windows".into(),
        ))
    }

    fn click_region(
        &self,
        _region: crate::core::types::Rect,
        _app: &AppTarget,
        _window: Option<&WindowTarget>,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "click_region not yet implemented on Windows".into(),
        ))
    }

    fn hover_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        _app: &AppTarget,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "hover not yet implemented on Windows (ref: {reference})"
        )))
    }

    fn hover_at_point(
        &self,
        _point: crate::core::types::Point,
        _app: Option<&AppTarget>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "hover not yet implemented on Windows".into(),
        ))
    }

    fn hover_region(
        &self,
        _region: crate::core::types::Rect,
        _app: &AppTarget,
        _window: Option<&WindowTarget>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "hover_region not yet implemented on Windows".into(),
        ))
    }

    fn ocr_hover(
        &self,
        _text: &str,
        _app: &AppTarget,
        _window: Option<&WindowTarget>,
        _index: Option<usize>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "ocr_hover not yet implemented on Windows".into(),
        ))
    }

    fn type_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        _text: &str,
        _app: &AppTarget,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "type not yet implemented on Windows (ref: {reference})"
        )))
    }

    fn keyboard_type(
        &self,
        _text: &str,
        _app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "keyboard_type not yet implemented on Windows".into(),
        ))
    }

    fn press(
        &self,
        _keys: &crate::core::key_combo::KeyCombo,
        _app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "press not yet implemented on Windows".into(),
        ))
    }

    fn scroll(
        &self,
        _direction: &str,
        _amount: u32,
        _app: &AppTarget,
        _window: Option<&WindowTarget>,
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
        _app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "drag not yet implemented on Windows".into(),
        ))
    }

    fn drag_refs(
        &self,
        _from: crate::core::element_tree::ElementRef,
        _to: crate::core::element_tree::ElementRef,
        _app: &AppTarget,
        _options: &crate::core::key_combo::DragOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "drag not yet implemented on Windows".into(),
        ))
    }

    fn ocr_click(
        &self,
        _text: &str,
        _app: &AppTarget,
        _window: Option<&WindowTarget>,
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
        _app: &AppTarget,
        _window: Option<&WindowTarget>,
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
        _app: &AppTarget,
    ) -> Result<crate::core::types::Point, ForepawError> {
        Err(ForepawError::ActionFailed(
            "resolve_ref_position not yet implemented on Windows".into(),
        ))
    }

    fn resolve_ref_bounds(
        &self,
        _ref: crate::core::element_tree::ElementRef,
        _app: &AppTarget,
    ) -> Result<crate::core::types::Rect, ForepawError> {
        Err(ForepawError::ActionFailed(
            "resolve_ref_bounds not yet implemented on Windows".into(),
        ))
    }

    fn element_at_point(
        &self,
        point: crate::core::types::Point,
        app_hint: Option<&AppTarget>,
    ) -> Result<crate::platform::HitTestResult, ForepawError> {
        hit_test::element_at_point(point, app_hint)
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

    fn get_text_attributes(
        &self,
        _app: &AppTarget,
        _reference: crate::core::element_tree::ElementRef,
    ) -> Result<Option<crate::core::text_attrs::TextAttrsResult>, ForepawError> {
        Ok(None)
    }
}
