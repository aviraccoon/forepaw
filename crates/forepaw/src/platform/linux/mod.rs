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

pub mod action;
pub mod app;
pub mod compositor;
pub mod hit_test;
pub mod input;
pub mod key_code;
pub mod role;
pub mod snapshot;
pub mod state;

use std::sync::Mutex;

use crate::core::errors::ForepawError;
use crate::platform::{AppTarget, DesktopProvider, WindowTarget};

use snapshot::AtspiRef;
use snapshot::RefHandleMap;

/// Linux implementation of `DesktopProvider`.
///
/// Connects to the AT-SPI2 accessibility bus (discovered from the
/// session bus at `org.a11y.Bus`) and uses `zbus::blocking` for
/// all D-Bus communication.
///
/// Holds a ref→handle cache from the last snapshot for O(1) ref resolution.
/// Connections themselves are still established per-call, so the binary can be
/// inspected (`--help`, `--version`) without requiring an AT-SPI2 bus.
#[derive(Debug)]
pub struct LinuxProvider {
    /// Ref→`(bus, path)` cache from the last snapshot. Replaced wholesale on
    /// each snapshot.
    ref_handles: Mutex<RefHandleMap>,
    /// Lazily-created `/dev/uinput` device, held for the session so the
    /// compositor-recognition delay is paid once (under the first action's
    /// setup). `None` until the first raw-input action.
    uinput: Mutex<Option<input::UinputDevice>>,
}

impl LinuxProvider {
    /// Creates a new `LinuxProvider`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ref_handles: Mutex::new(RefHandleMap::empty()),
            uinput: Mutex::new(None),
        }
    }

    /// Look up the retained handle for `ref_id` from the last snapshot's cache.
    fn cached_handle(&self, ref_id: i32) -> Option<AtspiRef> {
        self.ref_handles
            .lock()
            .expect("ref_handles mutex poisoned")
            .get(ref_id)
    }

    /// Run `f` with the lazily-created uinput device. The device (and its
    /// compositor-recognition settle) is created on first use and held for the
    /// session. The lock is held for the duration of `f`, which is fine for the
    /// synchronous, single-caller trait usage.
    fn with_uinput<R>(
        &self,
        f: impl FnOnce(&input::UinputDevice) -> Result<R, ForepawError>,
    ) -> Result<R, ForepawError> {
        let mut guard = self.uinput.lock().expect("uinput mutex poisoned");
        if guard.is_none() {
            *guard = Some(input::UinputDevice::open()?);
        }
        f(guard.as_ref().expect("uinput device just initialized"))
    }

    /// Lock the uinput slot and lazily create the device best-effort, returning
    /// a guard that yields `Option<&UinputDevice>`. Used by `click_ref`, whose
    /// AT-SPI2 `DoAction` path needs no uinput and must stay usable without
    /// `/dev/uinput` access (e.g. SSH sessions with no perms). Creation errors
    /// are swallowed: a missing device only matters for the coordinate
    /// fallback, which surfaces a clear error via the action layer.
    fn uinput_slot(&self) -> std::sync::MutexGuard<'_, Option<input::UinputDevice>> {
        let mut guard = self.uinput.lock().expect("uinput mutex poisoned");
        if guard.is_none() {
            if let Ok(dev) = input::UinputDevice::open() {
                *guard = Some(dev);
            }
        }
        guard
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

    fn displays(&self) -> Result<Vec<crate::core::display::DisplayInfo>, ForepawError> {
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
        let (tree, ref_handles) = snapshot::snapshot(app, window, options)?;
        *self.ref_handles.lock().expect("ref_handles mutex poisoned") = ref_handles;
        Ok(tree)
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

    // --- Actions ---

    fn activate_app(&self, app: &AppTarget) -> Result<(), ForepawError> {
        action::activate(app)
    }

    fn click_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        app: &AppTarget,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        let cached = self.cached_handle(reference.id);
        let guard = self.uinput_slot();
        action::click_ref(reference, app, options, cached, guard.as_ref())
    }

    fn click_at_point(
        &self,
        point: crate::core::types::Point,
        app: &AppTarget,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        self.with_uinput(|dev| action::click_at_point(point, app, options, dev))
    }

    fn click_region(
        &self,
        region: crate::core::types::Rect,
        app: &AppTarget,
        _window: Option<&WindowTarget>,
        options: &crate::core::key_combo::ClickOptions,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        self.with_uinput(|dev| action::click_region(region, app, options, dev))
    }

    fn hover_ref(
        &self,
        reference: crate::core::element_tree::ElementRef,
        app: &AppTarget,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        let cached = self.cached_handle(reference.id);
        self.with_uinput(|dev| action::hover_ref(reference, app, cached, dev))
    }

    fn hover_at_point(
        &self,
        point: crate::core::types::Point,
        app: Option<&AppTarget>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        self.with_uinput(|dev| action::hover_at_point(point, app, dev))
    }

    fn hover_region(
        &self,
        region: crate::core::types::Rect,
        app: &AppTarget,
        _window: Option<&WindowTarget>,
        _smooth: bool,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        self.with_uinput(|dev| action::hover_region(region, app, dev))
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
        text: &str,
        app: &AppTarget,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        let cached = self.cached_handle(reference.id);
        action::type_ref(reference, text, app, cached)
    }

    fn keyboard_type(
        &self,
        text: &str,
        app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        if let Some(app) = app {
            action::activate(app)?;
        }
        let msg = self.with_uinput(|dev| input::type_text(dev, text))?;
        Ok(crate::platform::ActionResult::ok_msg(msg))
    }

    fn press(
        &self,
        keys: &crate::core::key_combo::KeyCombo,
        app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        if let Some(app) = app {
            action::activate(app)?;
        }
        self.with_uinput(|dev| input::press(dev, keys))?;
        Ok(crate::platform::ActionResult::ok())
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
        path: &[crate::core::types::Point],
        options: &crate::core::key_combo::DragOptions,
        app: Option<&AppTarget>,
    ) -> Result<crate::platform::ActionResult, ForepawError> {
        self.with_uinput(|dev| action::drag_path(path, options, app, dev))
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
        self.with_uinput(|dev| {
            action::drag_refs(from, to, app, options, from_cached, to_cached, dev)
        })
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
        reference: crate::core::element_tree::ElementRef,
        app: &AppTarget,
    ) -> Result<crate::core::types::Point, ForepawError> {
        let cached = self.cached_handle(reference.id);
        action::resolve_ref_position(reference, app, cached)
    }

    fn resolve_ref_bounds(
        &self,
        reference: crate::core::element_tree::ElementRef,
        app: &AppTarget,
    ) -> Result<crate::core::types::Rect, ForepawError> {
        let cached = self.cached_handle(reference.id);
        action::resolve_ref_bounds(reference, app, cached)
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
