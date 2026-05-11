/// Platform abstraction for desktop automation.
///
/// Each backend implements the `DesktopProvider` trait.
/// The correct backend is selected via cfg attributes.
#[cfg(target_os = "macos")]
pub mod darwin;
use crate::core::annotation::{Annotation, AnnotationStyle};
use crate::core::crop_region::CropRegion;
use crate::core::element_tree::ElementRef;
use crate::core::errors::ForepawError;
use crate::core::key_combo::{ClickOptions, DragOptions, KeyCombo};
use crate::core::ocr_result::OCROutput;
use crate::core::types::{Point, Rect};

/// Info about a running application.
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: Option<String>,
    pub pid: i32,
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
    pub fn file_extension(&self) -> &str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::BestAvailable => "jpg", // fallback
        }
    }
}

/// Parameters for screenshot operations.
pub struct ScreenshotParams<'a> {
    pub app: Option<&'a str>,
    pub window: Option<&'a str>,
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

/// Options for snapshot (AX tree walk).
#[derive(Debug, Clone)]
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
    fn list_apps(&self) -> Result<Vec<AppInfo>, ForepawError>;
    fn list_windows(&self, app: Option<&str>) -> Result<Vec<WindowInfo>, ForepawError>;
    fn snapshot(
        &self,
        app: &str,
        options: &SnapshotOptions,
    ) -> Result<crate::core::element_tree::ElementTree, ForepawError>;

    fn screenshot(&self, params: &ScreenshotParams) -> Result<ScreenshotResult, ForepawError>;

    fn ocr(
        &self,
        app: Option<&str>,
        window: Option<&str>,
        find: Option<&str>,
        screenshot_options: Option<&ScreenshotOptions>,
    ) -> Result<OCROutput, ForepawError>;

    // Element-based actions
    fn click_ref(
        &self,
        r#ref: ElementRef,
        app: &str,
        options: &ClickOptions,
    ) -> Result<ActionResult, ForepawError>;
    fn click_at_point(
        &self,
        point: Point,
        app: &str,
        options: &ClickOptions,
    ) -> Result<ActionResult, ForepawError>;
    fn click_region(
        &self,
        region: Rect,
        app: &str,
        window: Option<&str>,
        options: &ClickOptions,
    ) -> Result<ActionResult, ForepawError>;

    fn hover_ref(&self, r#ref: ElementRef, app: &str) -> Result<ActionResult, ForepawError>;
    fn hover_at_point(
        &self,
        point: Point,
        app: Option<&str>,
        smooth: bool,
    ) -> Result<ActionResult, ForepawError>;
    fn hover_region(
        &self,
        region: Rect,
        app: &str,
        window: Option<&str>,
        smooth: bool,
    ) -> Result<ActionResult, ForepawError>;
    fn ocr_hover(
        &self,
        text: &str,
        app: &str,
        window: Option<&str>,
        index: Option<usize>,
    ) -> Result<ActionResult, ForepawError>;

    fn type_ref(
        &self,
        r#ref: ElementRef,
        text: &str,
        app: &str,
    ) -> Result<ActionResult, ForepawError>;
    fn keyboard_type(&self, text: &str, app: Option<&str>) -> Result<ActionResult, ForepawError>;
    fn press(&self, keys: &KeyCombo, app: Option<&str>) -> Result<ActionResult, ForepawError>;

    fn scroll(
        &self,
        direction: &str,
        amount: u32,
        app: &str,
        window: Option<&str>,
        r#ref: Option<ElementRef>,
        at: Option<Point>,
    ) -> Result<ActionResult, ForepawError>;

    fn drag_path(
        &self,
        path: &[Point],
        options: &DragOptions,
        app: Option<&str>,
    ) -> Result<ActionResult, ForepawError>;
    fn drag_refs(
        &self,
        from: ElementRef,
        to: ElementRef,
        app: &str,
        options: &DragOptions,
    ) -> Result<ActionResult, ForepawError>;

    fn ocr_click(
        &self,
        text: &str,
        app: &str,
        window: Option<&str>,
        options: &ClickOptions,
        index: Option<usize>,
    ) -> Result<ActionResult, ForepawError>;

    fn wait(
        &self,
        text: &str,
        app: &str,
        window: Option<&str>,
        timeout: f64,
        interval: f64,
    ) -> Result<ActionResult, ForepawError>;

    // Utility
    fn resolve_ref_position(&self, r#ref: ElementRef, app: &str) -> Result<Point, ForepawError>;
    fn resolve_ref_bounds(&self, r#ref: ElementRef, app: &str) -> Result<Rect, ForepawError>;

    // Permissions
    fn has_permissions(&self) -> bool;
    fn has_screen_recording_permission(&self) -> bool;
    fn validate_screen_recording(&self) -> bool;
    fn request_permissions(&self) -> bool;
    fn request_screen_recording_permission(&self) -> bool;
}
