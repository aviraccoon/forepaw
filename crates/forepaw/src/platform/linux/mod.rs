//! Linux backend using AT-SPI2 via D-Bus.
//!
//! Implements the `DesktopProvider` trait using:
//! - AT-SPI2 registry (`org.a11y.atspi.Registry`) for app enumeration
//! - `org.a11y.atspi.Accessible` interface for tree walking
//! - `org.a11y.atspi.Component` interface for element bounds
//! - `spectacle` (KDE) or `magick import` (X11) for screenshots
//! - `tesseract` CLI for OCR
//!
//! All D-Bus calls use `zbus::blocking` to stay synchronous (matching
//! the sync `DesktopProvider` trait). No async runtime needed.
//!
//! # AT-SPI2 bus discovery
//!
//! The AT-SPI2 bus is a separate D-Bus from the session bus. The address
//! is obtained by calling `org.a11y.Bus.GetAddress` on the session bus,
//! then connecting to that address for all subsequent AT-SPI2 calls.

pub mod app;
pub mod hit_test;
pub mod role;
pub mod snapshot;

use crate::core::errors::ForepawError;
use crate::platform::{AppTarget, DesktopProvider, WindowTarget};

/// Linux implementation of `DesktopProvider`.
///
/// Connects to the AT-SPI2 accessibility bus (discovered from the
/// session bus at `org.a11y.Bus`) and uses `zbus::blocking` for
/// all D-Bus communication.
#[derive(Debug)]
pub struct LinuxProvider {
    // No cached connection -- connections are established per-call
    // so the binary can be inspected (`--help`, `--version`)
    // without requiring an AT-SPI2 bus.
}

impl LinuxProvider {
    /// Creates a new `LinuxProvider`.
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for LinuxProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopProvider for LinuxProvider {
    // --- Observation ---

    fn list_apps(&self) -> Result<Vec<crate::platform::AppInfo>, ForepawError> {
        app::list_apps()
    }

    fn list_windows(
        &self,
        app: Option<&AppTarget>,
    ) -> Result<Vec<crate::platform::WindowInfo>, ForepawError> {
        app::list_windows(app)
    }

    fn displays(&self) -> Result<Vec<crate::platform::DisplayInfo>, ForepawError> {
        Err(ForepawError::ActionFailed(
            "displays not yet implemented on Linux".into(),
        ))
    }

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
        _params: &crate::platform::ScreenshotParams,
    ) -> Result<crate::platform::ScreenshotResult, ForepawError> {
        // TODO: shell out to spectacle (KDE) or magick import (X11)
        Err(ForepawError::ActionFailed(
            "screenshot not yet implemented on Linux".into(),
        ))
    }

    fn ocr(
        &self,
        _app: Option<&AppTarget>,
        _window: Option<&WindowTarget>,
        _find: Option<&str>,
        _screenshot_options: Option<&crate::core::encoder_detection::ScreenshotOptions>,
    ) -> Result<crate::core::ocr_result::OCROutput, ForepawError> {
        // TODO: shell out to tesseract CLI
        Err(ForepawError::ActionFailed(
            "ocr not yet implemented on Linux".into(),
        ))
    }

    // --- Actions (stubs) ---

    fn click_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        _app: &AppTarget,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "click not yet implemented on Linux (ref: {reference})"
        )))
    }

    fn click_at_point(
        &self,
        _point: crate::core::types::Point,
        _app: &AppTarget,
        _options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "click not yet implemented on Linux".into(),
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
            "click_region not yet implemented on Linux".into(),
        ))
    }

    fn hover_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        _app: &AppTarget,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "hover not yet implemented on Linux (ref: {reference})"
        )))
    }

    fn hover_at_point(
        &self,
        _point: crate::core::types::Point,
        _app: Option<&AppTarget>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "hover not yet implemented on Linux".into(),
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
            "hover_region not yet implemented on Linux".into(),
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
            "ocr_hover not yet implemented on Linux".into(),
        ))
    }

    fn type_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        _text: &str,
        _app: &AppTarget,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(format!(
            "type not yet implemented on Linux (ref: {reference})"
        )))
    }

    fn keyboard_type(
        &self,
        _text: &str,
        _app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "keyboard_type not yet implemented on Linux".into(),
        ))
    }

    fn press(
        &self,
        _keys: &crate::core::key_combo::KeyCombo,
        _app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "press not yet implemented on Linux".into(),
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
            "scroll not yet implemented on Linux".into(),
        ))
    }

    fn drag_path(
        &self,
        _path: &[crate::core::types::Point],
        _options: &crate::core::key_combo::DragOptions,
        _app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        Err(ForepawError::ActionFailed(
            "drag not yet implemented on Linux".into(),
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
            "drag not yet implemented on Linux".into(),
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
            "ocr_click not yet implemented on Linux".into(),
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
            "wait not yet implemented on Linux".into(),
        ))
    }

    // --- Utility ---

    fn resolve_ref_position(
        &self,
        _ref: crate::core::element_tree::ElementRef,
        _app: &AppTarget,
    ) -> Result<crate::core::types::Point, ForepawError> {
        Err(ForepawError::ActionFailed(
            "resolve_ref_position not yet implemented on Linux".into(),
        ))
    }

    fn resolve_ref_bounds(
        &self,
        _ref: crate::core::element_tree::ElementRef,
        _app: &AppTarget,
    ) -> Result<crate::core::types::Rect, ForepawError> {
        Err(ForepawError::ActionFailed(
            "resolve_ref_bounds not yet implemented on Linux".into(),
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
    // Linux has no equivalent of macOS Accessibility / Screen Recording gates.
    // AT-SPI2 works without special permissions (any user process can connect).
    // X11/Wayland screen capture may need compositor-specific permissions.

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

    fn get_text_attributes_by_uid(
        &self,
        _uid: u64,
    ) -> Result<Option<crate::core::text_attrs::TextAttrsResult>, ForepawError> {
        Ok(None)
    }
}
