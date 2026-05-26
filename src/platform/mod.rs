/// Platform abstraction for desktop automation.
///
/// Each backend implements the `DesktopProvider` trait.
/// The correct backend is selected via cfg attributes.
#[cfg(target_os = "macos")]
pub mod darwin;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;
use crate::core::annotation::{Annotation, AnnotationStyle};
use crate::core::crop_region::CropRegion;
use crate::core::element_tree::ElementRef;
use crate::core::errors::ForepawError;
use crate::core::key_combo::{ClickOptions, DragOptions, KeyCombo};
use crate::core::ocr_result::OCROutput;
use crate::core::role::Role;
use crate::core::types::{Point, Rect};

/// Identifies a target application unambiguously.
///
/// `Name` uses the platform's fuzzy resolution (display name, bundle ID,
/// process name, partial match). `Pid` targets a specific process.
///
/// CLI maps `--app` → `Name` and `--pid` → `Pid`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppTarget {
    /// Application name (display name, bundle ID, or partial match).
    Name(String),
    /// Process ID.
    Pid(i32),
}

impl AppTarget {
    /// Create a name-based target.
    #[must_use]
    pub fn name(s: impl Into<String>) -> Self {
        Self::Name(s.into())
    }

    /// Create a PID-based target.
    #[must_use]
    pub fn pid(n: i32) -> Self {
        Self::Pid(n)
    }

    /// Return a display string for error messages.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Name(s) => s.clone(),
            Self::Pid(n) => format!("pid {n}"),
        }
    }

    /// Return the name if this is a `Name` target.
    #[must_use]
    pub fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(s) => Some(s),
            Self::Pid(_) => None,
        }
    }

    /// Return the PID if this is a `Pid` target.
    #[must_use]
    pub fn as_pid(&self) -> Option<i32> {
        match self {
            Self::Name(_) => None,
            Self::Pid(n) => Some(*n),
        }
    }

    /// Cache key: the name string, or `"pid:{n}"` for PID targets.
    #[must_use]
    pub fn cache_key(&self) -> String {
        match self {
            Self::Name(s) => s.clone(),
            Self::Pid(n) => format!("pid:{n}"),
        }
    }
}

impl std::fmt::Display for AppTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(s) => write!(f, "{s}"),
            Self::Pid(n) => write!(f, "pid {n}"),
        }
    }
}

/// Info about a running application.
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: Option<String>,
    pub pid: i32,
}

/// How to target a specific window within an app.
#[derive(Debug, Clone)]
pub enum WindowTarget {
    /// Case-insensitive substring match on window title.
    Title(String),
    /// Exact window ID (e.g. "w-1234" from `list-windows`).
    Id(String),
}

impl WindowTarget {
    /// Create a title-based window target.
    #[must_use]
    pub fn title(title: impl Into<String>) -> Self {
        Self::Title(title.into())
    }

    /// Create an ID-based window target.
    #[must_use]
    pub fn id(id: impl Into<String>) -> Self {
        Self::Id(id.into())
    }

    /// Display string for error messages.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Title(t) => format!("title:{t}"),
            Self::Id(id) => format!("id:{id}"),
        }
    }
}

/// Info about a visible window.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub app: String,
    pub bounds: Option<Rect>,
}

/// Result of an action (click, type, press, etc.).
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub success: bool,
    pub message: Option<String>,
}

impl ActionResult {
    #[must_use]
    pub fn ok() -> Self {
        Self {
            success: true,
            message: None,
        }
    }

    pub fn ok_msg(msg: impl Into<String>) -> Self {
        Self {
            success: true,
            message: Some(msg.into()),
        }
    }

    pub fn fail(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            message: Some(msg.into()),
        }
    }
}

/// An element in the ancestor chain of a hit-test result.
#[derive(Debug, Clone)]
pub struct AncestorInfo {
    /// The element role.
    pub role: Role,
    /// The element's accessible name, if any.
    pub name: Option<String>,
    /// Bounds in screen coordinates.
    pub bounds: Option<Rect>,
}

/// Result of a hit test at a screen coordinate.
///
/// Contains the deepest element at the point and its ancestor chain,
/// ordered root-first (index 0 = root application element).
/// The element is the leaf hit by the platform hit-test API
/// (e.g. `AXUIElementCopyElementAtPosition` returns the deepest child).
#[derive(Debug, Clone)]
pub struct HitTestResult {
    pub role: Role,
    pub name: Option<String>,
    pub value: Option<String>,
    pub bounds: Option<Rect>,
    /// Available actions on this element (e.g. `Press`, `ShowMenu`).
    /// Platform-specific: `AXActionNames` on macOS, Control Patterns on Windows,
    /// Action names on AT-SPI2.
    pub actions: Vec<String>,
    /// Ancestor chain from root to parent, ordered root-first.
    pub ancestors: Vec<AncestorInfo>,
    /// PID of the owning application.
    pub pid: i32,
}

/// Options controlling screenshot output format and quality.
#[derive(Debug, Clone)]
pub struct ScreenshotOptions {
    pub format: ImageFormat,
    pub quality: u32,
    pub scale: u32,
    pub cursor: bool,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            format: ImageFormat::BestAvailable,
            quality: 85,
            scale: 1,
            cursor: true,
        }
    }
}

/// Image format for screenshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    BestAvailable,
}

impl std::str::FromStr for ImageFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "png" => Ok(Self::Png),
            "jpeg" | "jpg" => Ok(Self::Jpeg),
            "webp" => Ok(Self::Webp),
            _ => Err(()),
        }
    }
}

impl ImageFormat {
    #[must_use]
    pub fn file_extension(&self) -> &str {
        match self {
            Self::Png => "png",
            Self::Jpeg | Self::BestAvailable => "jpg",
            Self::Webp => "webp",
        }
    }
}

/// Parameters for screenshot operations.
pub struct ScreenshotParams<'a> {
    pub app: Option<&'a AppTarget>,
    pub window: Option<&'a WindowTarget>,
    pub style: Option<AnnotationStyle>,
    pub only: Option<&'a [ElementRef]>,
    pub options: &'a ScreenshotOptions,
    pub crop: Option<&'a CropRegion>,
    pub grid_spacing: Option<u32>,
}

/// Result of a screenshot operation.
#[derive(Debug, Clone)]
pub struct ScreenshotResult {
    pub path: String,
    pub annotations: Option<Vec<Annotation>>,
    pub legend: Option<String>,
}

/// Options for snapshot (accessibility tree walk).
#[derive(Debug, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "snapshot options accumulate flags"
)]
pub struct SnapshotOptions {
    pub interactive_only: bool,
    pub max_depth: usize,
    pub compact: bool,
    pub skip_menu_bar: bool,
    pub skip_zero_size: bool,
    pub skip_offscreen: bool,
    pub window_bounds: Option<Rect>,
    pub timing: bool,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            interactive_only: false,
            max_depth: 15,
            compact: false,
            skip_menu_bar: false,
            skip_zero_size: false,
            skip_offscreen: false,
            window_bounds: None,
            timing: false,
        }
    }
}

impl SnapshotOptions {
    pub const DEFAULT_DEPTH: usize = 15;
}

/// Platform abstraction trait.
///
/// The CLI talks exclusively through this trait. Platform backends
/// implement it with their native APIs.
pub trait DesktopProvider: Send + Sync {
    // Observation

    /// Returns all running GUI applications.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::PermissionDenied`] if accessibility access is not granted.
    fn list_apps(&self) -> Result<Vec<AppInfo>, ForepawError>;

    /// Returns visible windows, optionally filtered by application name.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if `app` is provided but no matching
    /// process is found.
    fn list_windows(&self, app: Option<&AppTarget>) -> Result<Vec<WindowInfo>, ForepawError>;

    /// Walks the accessibility tree of the given application.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if the application is not running, or
    /// [`ForepawError::PermissionDenied`] if accessibility access is not granted.
    fn snapshot(
        &self,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        options: &SnapshotOptions,
    ) -> Result<crate::core::element_tree::ElementTree, ForepawError>;

    /// Captures a screenshot, optionally annotated with element labels.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if the target application is not running,
    /// [`ForepawError::ScreenRecordingDenied`] if screen recording permission is missing,
    /// or [`ForepawError::StaleRef`] if a ref filter targets a non-existent element.
    fn screenshot(&self, params: &ScreenshotParams) -> Result<ScreenshotResult, ForepawError>;

    /// Runs OCR on a screenshot of the target app/window.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if the target application is not running,
    /// [`ForepawError::ScreenRecordingDenied`] if screen recording permission is missing,
    /// or [`ForepawError::WindowNotFound`] if a window filter doesn't match.
    fn ocr(
        &self,
        app: Option<&AppTarget>,
        window: Option<&WindowTarget>,
        find: Option<&str>,
        screenshot_options: Option<&ScreenshotOptions>,
    ) -> Result<OCROutput, ForepawError>;

    // Element-based actions

    /// Clicks an element identified by its ref from a prior snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if the application is not running,
    /// [`ForepawError::StaleRef`] if the ref no longer exists in the tree,
    /// or [`ForepawError::PermissionDenied`] if accessibility access is not granted.
    fn click_ref(
        &self,
        r#ref: ElementRef,
        app: &AppTarget,
        options: &ClickOptions,
    ) -> Result<ActionResult, ForepawError>;

    /// Clicks at absolute screen coordinates within the target app's window.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if the application is not running,
    /// [`ForepawError::WindowNotFound`] if the window cannot be resolved,
    /// or [`ForepawError::ActionFailed`] if the point falls outside the window bounds.
    fn click_at_point(
        &self,
        point: Point,
        app: &AppTarget,
        options: &ClickOptions,
    ) -> Result<ActionResult, ForepawError>;

    /// Clicks the most visually salient point within a bounding region.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if the application is not running,
    /// [`ForepawError::WindowNotFound`] if the window cannot be resolved,
    /// or [`ForepawError::ActionFailed`] if saliency analysis fails.
    fn click_region(
        &self,
        region: Rect,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        options: &ClickOptions,
    ) -> Result<ActionResult, ForepawError>;

    /// Hovers over an element identified by its ref.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree,
    /// or [`ForepawError::ActionFailed`] if the element has no position or size.
    fn hover_ref(&self, r#ref: ElementRef, app: &AppTarget) -> Result<ActionResult, ForepawError>;

    /// Moves the cursor to absolute screen coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if the platform input API rejects the event.
    fn hover_at_point(
        &self,
        point: Point,
        app: Option<&AppTarget>,
        smooth: bool,
    ) -> Result<ActionResult, ForepawError>;

    /// Hovers over the most visually salient point within a bounding region.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if the application is not running,
    /// [`ForepawError::WindowNotFound`] if the window cannot be resolved,
    /// or [`ForepawError::ActionFailed`] if saliency analysis fails.
    fn hover_region(
        &self,
        region: Rect,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        smooth: bool,
    ) -> Result<ActionResult, ForepawError>;

    /// Hovers at the position of OCR-recognized text.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if the text is not found in OCR results,
    /// or if multiple matches exist but no index is specified.
    fn ocr_hover(
        &self,
        text: &str,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        index: Option<usize>,
    ) -> Result<ActionResult, ForepawError>;

    /// Types text into an element identified by its ref via the accessibility API.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree,
    /// or [`ForepawError::ActionFailed`] if the element does not support text input.
    fn type_ref(
        &self,
        r#ref: ElementRef,
        text: &str,
        app: &AppTarget,
    ) -> Result<ActionResult, ForepawError>;

    /// Types text via simulated keyboard events into whatever has focus.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if the platform input API rejects the events.
    fn keyboard_type(
        &self,
        text: &str,
        app: Option<&AppTarget>,
    ) -> Result<ActionResult, ForepawError>;

    /// Presses a key combination (e.g. Cmd+S) via simulated keyboard events.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if the key combination is invalid
    /// or the platform input API rejects the events.
    fn press(&self, keys: &KeyCombo, app: Option<&AppTarget>)
        -> Result<ActionResult, ForepawError>;

    /// Scrolls the content of the target window.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::AppNotFound`] if the application is not running,
    /// [`ForepawError::StaleRef`] if a ref is provided but no longer exists in the tree,
    /// or [`ForepawError::ActionFailed`] if the scroll event cannot be posted.
    fn scroll(
        &self,
        direction: &str,
        amount: u32,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        r#ref: Option<ElementRef>,
        at: Option<Point>,
    ) -> Result<ActionResult, ForepawError>;

    /// Drags along a path of screen coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if the path is too short,
    /// or the platform input API rejects the drag events.
    fn drag_path(
        &self,
        path: &[Point],
        options: &DragOptions,
        app: Option<&AppTarget>,
    ) -> Result<ActionResult, ForepawError>;

    /// Drags from one element to another, identified by refs.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::StaleRef`] if either ref no longer exists in the tree,
    /// or [`ForepawError::ActionFailed`] if an element has no position or size.
    fn drag_refs(
        &self,
        from: ElementRef,
        to: ElementRef,
        app: &AppTarget,
        options: &DragOptions,
    ) -> Result<ActionResult, ForepawError>;

    /// Clicks at the position of OCR-recognized text.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if the text is not found in OCR results,
    /// or if multiple matches exist but no index is specified.
    fn ocr_click(
        &self,
        text: &str,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        options: &ClickOptions,
        index: Option<usize>,
    ) -> Result<ActionResult, ForepawError>;

    /// Polls OCR until the given text appears or the timeout elapses.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if the text is not found before the timeout,
    /// or [`ForepawError::ScreenRecordingDenied`] if screen recording permission is missing.
    fn wait(
        &self,
        text: &str,
        app: &AppTarget,
        window: Option<&WindowTarget>,
        timeout: f64,
        interval: f64,
    ) -> Result<ActionResult, ForepawError>;

    // Utility

    /// Resolves an element ref to its center point in screen coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree,
    /// or [`ForepawError::ActionFailed`] if the element has no position or size.
    fn resolve_ref_position(
        &self,
        r#ref: ElementRef,
        app: &AppTarget,
    ) -> Result<Point, ForepawError>;

    /// Resolves an element ref to its bounding rectangle in screen coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree,
    /// or [`ForepawError::ActionFailed`] if the element has no position or size.
    fn resolve_ref_bounds(&self, r#ref: ElementRef, app: &AppTarget) -> Result<Rect, ForepawError>;

    /// Performs a hit test at the given screen coordinates.
    ///
    /// Returns the deepest accessibility element at the point and its ancestor chain.
    /// When `app_hint` is `None`, searches all running applications (system-wide).
    /// When scoped to an app name, only searches within that application.
    ///
    /// # Errors
    ///
    /// Returns [`ForepawError::ActionFailed`] if no element is found at the given point
    /// or the platform accessibility API fails.
    fn element_at_point(
        &self,
        point: Point,
        app_hint: Option<&AppTarget>,
    ) -> Result<HitTestResult, ForepawError>;

    // Permissions
    fn has_permissions(&self) -> bool;
    fn has_screen_recording_permission(&self) -> bool;
    fn validate_screen_recording(&self) -> bool;
    fn request_permissions(&self) -> bool;
    fn request_screen_recording_permission(&self) -> bool;
}
