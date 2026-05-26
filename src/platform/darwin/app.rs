//! Application and window lookup via `NSWorkspace` and `CGWindowList`.
//!
//! Provides the foundational lookup functions used by snapshot, input, and
//! screenshot modules. All functions return platform-agnostic types from
//! `crate::platform` or `crate::core::types`.

use std::collections::HashSet;
use std::path::Path;

use objc2::rc::Retained;
use objc2::Message;
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

use crate::core::errors::ForepawError;
use crate::core::types::{Point, Rect};
use crate::platform::darwin::ffi::{
    kCGWindowBounds, kCGWindowLayer, kCGWindowName, kCGWindowNumber, kCGWindowOwnerName,
    kCGWindowOwnerPID, AXIsProcessTrusted, AXUIElementCreateApplication, AXUIElementRef,
    AXUIElementSetAttributeValue, CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef,
    CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef, CFGetTypeID, CFIndex,
    CFNumberGetTypeID, CFNumberGetValue, CFNumberRef, CFRelease, CFStringGetCString,
    CFStringGetCStringPtr, CFStringGetTypeID, CFStringRef, CFTypeRef, CGDisplayBounds,
    CGGetOnlineDisplayList, CGWindowListCopyWindowInfo, CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY,
    K_CF_NUMBER_DOUBLE_TYPE, K_CF_NUMBER_SINT32_TYPE, K_CF_STRING_ENCODING_UTF8,
    K_CG_NULL_WINDOW_ID,
};
use crate::platform::darwin::snapshot::{fetch_batch_attributes, ATTR_CHILDREN, ATTR_ROLE};
use crate::platform::{AppInfo, WindowInfo, WindowState, WindowTarget};

// ---------------------------------------------------------------------------
// Permission gating
// ---------------------------------------------------------------------------

/// Check accessibility permission. Returns error if not granted.
fn require_accessibility() -> Result<(), ForepawError> {
    // SAFETY: AXIsProcessTrusted is a read-only system call.
    if unsafe { AXIsProcessTrusted() } == 0 {
        Err(ForepawError::PermissionDenied)
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Window state detection
// ---------------------------------------------------------------------------

/// Collect bounding rects of all online displays.
///
/// Returns empty vec if the CG call fails (shouldn't happen in practice).
fn screen_rects() -> Vec<Rect> {
    let max_displays = 16_u32;
    let mut display_ids = [0u32; 16];
    let mut display_count = 0_u32;
    // SAFETY: CGGetOnlineDisplayList writes to caller-provided buffers.
    let result = unsafe {
        CGGetOnlineDisplayList(
            max_displays,
            display_ids.as_mut_ptr(),
            &raw mut display_count,
        )
    };
    if result != 0 {
        return Vec::new();
    }
    let count = usize::try_from(display_count).unwrap_or(0);
    let mut rects = Vec::with_capacity(count);
    for &display_id in display_ids.iter().take(count) {
        // SAFETY: CGDisplayBounds returns a CGRectFFI for a valid display ID.
        let cg_rect = unsafe { CGDisplayBounds(display_id) };
        rects.push(Rect::new(
            cg_rect.origin.x,
            cg_rect.origin.y,
            cg_rect.size.width,
            cg_rect.size.height,
        ));
    }
    rects
}

/// Detect window state by comparing bounds to screen rects.
///
/// macOS `CGWindowListCopyWindowInfo` with `ON_SCREEN_ONLY` excludes
/// minimized windows, so we only distinguish Normal vs Fullscreen.
/// A window is fullscreen if its bounds cover an entire screen.
fn detect_window_state(bounds: &Rect, layer: Option<i32>) -> WindowState {
    // Only layer-0 windows (normal app windows) can be fullscreen.
    // Other layers are system chrome (menu bar, Dock), floating panels,
    // overlays, or background windows that happen to be screen-sized.
    if layer != Some(0) {
        return WindowState::Normal;
    }
    for screen in &screen_rects() {
        // Allow 2px tolerance for rounding / decoration edge cases
        let covers_x = (bounds.x - screen.x).abs() <= 2.0;
        let covers_y = (bounds.y - screen.y).abs() <= 2.0;
        let covers_w = (bounds.width - screen.width).abs() <= 2.0;
        let covers_h = (bounds.height - screen.height).abs() <= 2.0;
        if covers_x && covers_y && covers_w && covers_h {
            return WindowState::Fullscreen;
        }
    }
    WindowState::Normal
}

// ---------------------------------------------------------------------------
// Application lookup
// ---------------------------------------------------------------------------

/// Find a running application by exact name, bundle ID, or case-insensitive
/// partial match (in that priority order).
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if no matching process is found,
/// or [`ForepawError::PermissionDenied`] if accessibility access is not granted.
pub fn find_app(name: &str) -> Result<Retained<NSRunningApplication>, ForepawError> {
    require_accessibility()?;
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();

    let count = apps.count();

    // Exact name match
    for i in 0..count {
        let app = apps.objectAtIndex(i);
        if let Some(localized) = app.localizedName() {
            if localized.to_string() == name {
                return Ok(app.retain());
            }
        }
    }

    // Bundle ID match
    for i in 0..count {
        let app = apps.objectAtIndex(i);
        if let Some(bundle_id) = app.bundleIdentifier() {
            if bundle_id.to_string() == name {
                return Ok(app.retain());
            }
        }
    }

    // Case-insensitive partial match
    let name_lower = name.to_lowercase();
    for i in 0..count {
        let app = apps.objectAtIndex(i);
        if let Some(localized) = app.localizedName() {
            if localized.to_string().to_lowercase().contains(&name_lower) {
                return Ok(app.retain());
            }
        }
    }

    Err(ForepawError::AppNotFound(name.to_owned()))
}

/// List all running applications with regular activation policy.
///
/// # Errors
///
/// Returns [`ForepawError::PermissionDenied`] if accessibility access is not granted.
pub fn list_apps() -> Result<Vec<AppInfo>, ForepawError> {
    require_accessibility()?;
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();
    let mut result = Vec::new();

    for i in 0..apps.count() {
        let app = apps.objectAtIndex(i);
        if app.activationPolicy() != objc2_app_kit::NSApplicationActivationPolicy::Regular {
            continue;
        }
        let name = match app.localizedName() {
            Some(n) => n.to_string(),
            None => continue,
        };
        let bundle_id = app.bundleIdentifier().map(|s| s.to_string());
        let pid = app.processIdentifier();
        let is_active = app.isActive();
        result.push(AppInfo {
            name,
            bundle_id,
            pid,
            is_active,
        });
    }

    Ok(result)
}
// ---------------------------------------------------------------------------

/// A resolved window with its `CGWindowID`, title, and bounds.
#[derive(Debug)]
pub struct ResolvedWindow {
    pub window_id: u32,
    pub title: String,
    pub bounds: Rect,
}

impl ResolvedWindow {
    #[must_use]
    pub fn origin(&self) -> Point {
        Point {
            x: self.bounds.x,
            y: self.bounds.y,
        }
    }

    #[must_use]
    pub fn center(&self) -> Point {
        Point {
            x: self.bounds.x + self.bounds.width / 2.0,
            y: self.bounds.y + self.bounds.height / 2.0,
        }
    }
}

/// Find the best-matching window for a given PID, optionally filtered by
/// window name or ID.
///
/// 1. Collect all on-screen windows for the PID (skip tiny/phantom windows)
/// 2. Multi-process fallback: if no windows found, try helper processes
///    sharing the same bundle ID prefix
/// 3. If `window` is specified:
///    - Match by window ID ("w-1234")
///    - Substring match on title (case-insensitive)
/// 4. Default: prefer titled windows, then largest by area
///
/// # Errors
///
/// Returns [`ForepawError::WindowNotFound`] if no window matches the filter,
/// or [`ForepawError::AmbiguousWindow`] if multiple windows match and none is preferred.
pub fn find_window(
    pid: i32,
    window: Option<&WindowTarget>,
) -> Result<ResolvedWindow, ForepawError> {
    require_accessibility()?;
    // SAFETY: CGWindowListCopyWindowInfo returns a CFArray the caller owns.
    let window_list = unsafe {
        CGWindowListCopyWindowInfo(CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY, K_CG_NULL_WINDOW_ID)
    };
    if window_list.is_null() {
        return Err(ForepawError::WindowNotFound(
            window.map_or_else(|| "any".to_owned(), WindowTarget::display),
        ));
    }

    let mut app_windows = windows_for_pid(window_list, pid);

    // Multi-process fallback: some apps (Steam) render UI in helper processes.
    if app_windows.is_empty() {
        if let Ok(main_app) = find_app_by_pid(pid) {
            if let Some(main_bundle) = main_app.bundleIdentifier() {
                let main_bundle_str = main_bundle.to_string();
                let helper_pids = collect_helper_pids(&main_bundle_str, pid);
                if !helper_pids.is_empty() {
                    app_windows = windows_for_pids(window_list, &helper_pids);
                }
            }
        }
    }

    // SAFETY: window_list is a valid CFType we own.
    unsafe { CFRelease(window_list as CFTypeRef) };

    if app_windows.is_empty() {
        return Err(ForepawError::WindowNotFound(
            window.map_or_else(|| "any".to_owned(), WindowTarget::display),
        ));
    }

    match window {
        Some(target) => match target {
            WindowTarget::Id(id) => match_window_by_id(&app_windows, id),
            WindowTarget::Title(title) => match_window_by_title(&app_windows, title),
        },
        None => Ok(select_best_window(&app_windows)),
    }
}

/// List visible windows, optionally filtered by app name.
///
/// Returns an error if accessibility permission is not granted, since
/// the window list would be incomplete without it.
///
/// # Errors
///
/// Returns [`ForepawError::PermissionDenied`] if accessibility access is not granted,
/// or [`ForepawError::AppNotFound`] if `app_name` is provided but no matching process is found.
pub fn list_windows(
    app: Option<&crate::platform::AppTarget>,
) -> Result<Vec<WindowInfo>, ForepawError> {
    // SAFETY: FFI call with valid arguments.
    let window_list = unsafe {
        CGWindowListCopyWindowInfo(CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY, K_CG_NULL_WINDOW_ID)
    };
    if window_list.is_null() {
        return Ok(Vec::new());
    }

    require_accessibility()?;

    // Build set of allowed PIDs if filtering by app
    let allowed_pids: Option<HashSet<i32>> = app.map(|target| match find_app_by_target(target) {
        Ok(app) => {
            let mut pids = HashSet::new();
            pids.insert(app.processIdentifier());
            if let Some(bundle_id) = app.bundleIdentifier() {
                let bundle_str = bundle_id.to_string();
                for helper_pid in collect_helper_pids(&bundle_str, app.processIdentifier()) {
                    pids.insert(helper_pid);
                }
            }
            pids
        }
        Err(_) => HashSet::new(),
    });

    // SAFETY: CFArrayGetCount on valid window_list.
    let count = unsafe { CFArrayGetCount(window_list) };
    let mut result = Vec::new();

    for i in 0..count {
        // SAFETY: index in bounds, CFArray is valid.
        let info = unsafe { CFArrayGetValueAtIndex(window_list, i as _) as CFDictionaryRef };
        if info.is_null() {
            continue;
        }

        // Filter by PID set if applicable
        if let Some(ref pids) = allowed_pids {
            // SAFETY: dict accessor on valid CFDictionary.
            let owner_pid = unsafe { get_dict_i32(info, kCGWindowOwnerPID) };
            match owner_pid {
                Some(pid) if pids.contains(&pid) => {}
                _ => continue,
            }
        }

        // SAFETY: dict accessor on valid CFDictionary.
        let owner_name = unsafe { get_dict_string(info, kCGWindowOwnerName) };
        // SAFETY: dict accessor on valid CFDictionary.
        let window_id = unsafe { get_dict_i32(info, kCGWindowNumber) };
        // SAFETY: dict accessor on valid CFDictionary.
        let title = unsafe { get_dict_string(info, kCGWindowName) }.unwrap_or_default();

        let (Some(owner), Some(id_num)) = (owner_name, window_id) else {
            continue;
        };

        // Filter by app name if no PID set was built (find_app failed)
        if allowed_pids.is_none() {
            if let Some(filter) = app.and_then(crate::platform::AppTarget::as_name) {
                if owner != filter {
                    continue;
                }
            }
        }

        // Skip phantom/tiny windows
        // SAFETY: dict accessor on valid CFDictionary.
        if let Some(bounds) = unsafe { get_dict_bounds(info, kCGWindowBounds) } {
            if bounds.width < 10.0 || bounds.height < 10.0 {
                continue;
            }
            // SAFETY: dict accessor on valid CFDictionary.
            let layer = unsafe { get_dict_i32(info, kCGWindowLayer) };
            let state = detect_window_state(&bounds, layer);
            result.push(WindowInfo {
                id: format!("w-{id_num}"),
                title,
                app: owner,
                bounds: Some(bounds),
                state: Some(state),
            });
        }
    }

    // SAFETY: CFRelease on a valid CFType we own.
    unsafe { CFRelease(window_list as CFTypeRef) };
    Ok(result)
}

// ---------------------------------------------------------------------------
// Coordinate conversion
// ---------------------------------------------------------------------------

/// Convert window-relative coordinates to screen-absolute coordinates.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the process has no accessible windows.
pub fn to_screen_point(point: &Point, pid: i32) -> Result<Point, ForepawError> {
    let resolved = find_window(pid, None)?;
    Ok(Point {
        x: point.x + resolved.bounds.x,
        y: point.y + resolved.bounds.y,
    })
}

/// Validate that a point is within the window bounds.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if the point falls outside the window.
pub fn validate_point_in_window(point: &Point, pid: i32) -> Result<(), ForepawError> {
    let resolved = find_window(pid, None)?;
    let w = resolved.bounds.width;
    let h = resolved.bounds.height;
    if point.x < 0.0 || point.y < 0.0 || point.x > w || point.y > h {
        return Err(ForepawError::ActionFailed(format!(
            "Point ({:.0}, {:.0}) is outside window bounds (0,0)-({:.0},{:.0})",
            point.x, point.y, w, h
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Electron detection
// ---------------------------------------------------------------------------

/// Check if an app bundle contains the Electron Framework.
/// CEF apps like Spotify are NOT included -- CEF doesn't respond to
/// `AXManualAccessibility` and exposes only empty group nodes.
pub fn is_electron_app(app: &NSRunningApplication) -> bool {
    let Some(bundle_url) = app.bundleURL() else {
        return false;
    };
    let path = bundle_url.path().unwrap_or_default();
    let framework_path = format!("{path}/Contents/Frameworks/Electron Framework.framework");
    Path::new(&framework_path).exists()
}

/// Tell an Electron app to build its Chromium accessibility tree.
/// Sets the `AXManualAccessibility` attribute on the app element.
pub fn enable_electron_accessibility(pid: i32) {
    // SAFETY: AXUIElementCreateApplication is a system call, no preconditions.
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    let attr_name = cf_string_from_str("AXManualAccessibility");
    let val = objc2_foundation::NSNumber::numberWithBool(true);
    let cf_val = Retained::as_ptr(&val) as CFTypeRef;
    // Prevent Drop -- the AX API doesn't retain it, but we need it alive
    // through the AXUIElementSetAttributeValue call.
    #[expect(
        clippy::mem_forget,
        reason = "keep NSNumber alive through AX call, Electron path unverified"
    )]
    std::mem::forget(val);
    // SAFETY: FFI calls on valid CoreGraphics/CoreFoundation objects.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "multiple FFI calls")]
    unsafe {
        AXUIElementSetAttributeValue(app_element, attr_name, cf_val);
        CFRelease(attr_name as CFTypeRef);
    }
}

/// Check if an Electron app's web content tree is populated.
/// Looks for an `AXWebArea` with any child elements.
#[must_use]
pub fn electron_tree_is_populated(pid: i32) -> bool {
    // SAFETY: AXUIElementCreateApplication is a system call, no preconditions.
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    has_populated_web_area(app_element, 0, 25)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

struct WindowEntry {
    id: u32,
    title: String,
    bounds: Rect,
}

/// Walk a `CGWindowListCopyWindowInfo` array, filtering by owner PID.
/// Returns windows with bounds >= 10x10.
///
/// # Safety
///
/// `window_list` must be a valid `CFArrayRef` from `CGWindowListCopyWindowInfo`.
unsafe fn collect_windows(
    window_list: CFArrayRef,
    pid_matches: impl Fn(i32) -> bool,
) -> Vec<WindowEntry> {
    // SAFETY: CFArrayGetCount on valid CFArray.
    let count = unsafe { CFArrayGetCount(window_list) };
    let mut entries = Vec::new();

    for i in 0..count {
        // SAFETY: index in bounds, CFArray is valid.
        let info = unsafe { CFArrayGetValueAtIndex(window_list, i as _) as CFDictionaryRef };
        if info.is_null() {
            continue;
        }
        // SAFETY: dict accessor on valid CFDictionary.
        let _owner_pid = match get_dict_i32(info, unsafe { kCGWindowOwnerPID }) {
            Some(p) if pid_matches(p) => p,
            _ => continue,
        };
        // SAFETY: accessing a global CFStringRef constant.
        let bounds_key = unsafe { kCGWindowBounds };
        let bounds = match get_dict_bounds(info, bounds_key) {
            Some(b) if b.width >= 10.0 && b.height >= 10.0 => b,
            _ => continue,
        };
        // SAFETY: dict accessor on valid CFDictionary.
        let window_id = get_dict_i32(info, unsafe { kCGWindowNumber })
            .and_then(|id| {
                #[expect(clippy::cast_sign_loss, reason = "window ID validated > 0 before cast")]
                if id > 0 {
                    Some(id as u32)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        // SAFETY: dict accessor on valid CFDictionary.
        let title = get_dict_string(info, unsafe { kCGWindowName }).unwrap_or_default();

        entries.push(WindowEntry {
            id: window_id,
            title,
            bounds,
        });
    }

    entries
}

fn windows_for_pid(window_list: CFArrayRef, pid: i32) -> Vec<WindowEntry> {
    // SAFETY: window_list comes from CGWindowListCopyWindowInfo.
    unsafe { collect_windows(window_list, |p| p == pid) }
}

fn windows_for_pids(window_list: CFArrayRef, pids: &HashSet<i32>) -> Vec<WindowEntry> {
    // SAFETY: window_list comes from CGWindowListCopyWindowInfo.
    unsafe { collect_windows(window_list, |p| pids.contains(&p)) }
}

fn match_window_by_id(windows: &[WindowEntry], id: &str) -> Result<ResolvedWindow, ForepawError> {
    // Accept bare numeric IDs ("1234") — validate as u32
    if let Ok(id_num) = id.parse::<u32>() {
        if let Some(w) = windows.iter().find(|w| w.id == id_num) {
            return Ok(ResolvedWindow {
                window_id: w.id,
                title: w.title.clone(),
                bounds: w.bounds,
            });
        }
    }
    Err(ForepawError::WindowNotFound(id.to_owned()))
}

/// Match windows by title substring (case-insensitive).
fn match_window_by_title(
    windows: &[WindowEntry],
    pattern: &str,
) -> Result<ResolvedWindow, ForepawError> {
    let pattern_lower = pattern.to_lowercase();
    let matches: Vec<&WindowEntry> = windows
        .iter()
        .filter(|w| w.title.to_lowercase().contains(&pattern_lower))
        .collect();

    match matches.len() {
        1 => {
            let m = matches.into_iter().next().expect("len == 1 checked");
            Ok(ResolvedWindow {
                window_id: m.id,
                title: m.title.clone(),
                bounds: m.bounds,
            })
        }
        2.. => {
            let titles = matches
                .iter()
                .map(|m| format!("  w-{}  {}", m.id, m.title))
                .collect::<Vec<_>>()
                .join("\n");
            Err(ForepawError::AmbiguousWindow {
                query: pattern.to_owned(),
                matches: titles,
            })
        }
        0 => Err(ForepawError::WindowNotFound(pattern.to_owned())),
    }
}

fn select_best_window(windows: &[WindowEntry]) -> ResolvedWindow {
    // Prefer titled windows, then largest by area.
    let titled: Vec<&WindowEntry> = windows.iter().filter(|w| !w.title.is_empty()).collect();
    let candidates = if titled.is_empty() {
        windows.iter().collect()
    } else {
        titled
    };

    let best = candidates
        .iter()
        .max_by(|a, b| {
            let area_a = a.bounds.width * a.bounds.height;
            let area_b = b.bounds.width * b.bounds.height;
            area_a
                .partial_cmp(&area_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("candidates non-empty (windows is non-empty)");

    ResolvedWindow {
        window_id: best.id,
        title: best.title.clone(),
        bounds: best.bounds,
    }
}

/// Find a running application by PID.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if no process with the given PID is found.
pub fn find_app_by_pid(pid: i32) -> Result<Retained<NSRunningApplication>, ForepawError> {
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();
    for i in 0..apps.count() {
        let app = apps.objectAtIndex(i);
        if app.processIdentifier() == pid {
            return Ok(app.retain());
        }
    }
    Err(ForepawError::AppNotFound(format!("pid {pid}")))
}

/// Find a running application by name or PID.
///
/// Delegates to [`find_app`] for [`AppTarget::Name`] or [`find_app_by_pid`]
/// for [`AppTarget::Pid`].
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if no matching process is found,
/// or [`ForepawError::PermissionDenied`] if accessibility access is not granted.
pub fn find_app_by_target(
    target: &crate::platform::AppTarget,
) -> Result<Retained<NSRunningApplication>, ForepawError> {
    match target {
        crate::platform::AppTarget::Name(name) => find_app(name),
        crate::platform::AppTarget::Pid(pid) => find_app_by_pid(*pid),
    }
}

fn collect_helper_pids(bundle_id: &str, main_pid: i32) -> HashSet<i32> {
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let apps = workspace.runningApplications();
    let mut pids = HashSet::new();

    for i in 0..apps.count() {
        let app = apps.objectAtIndex(i);
        if app.processIdentifier() == main_pid {
            continue;
        }
        if let Some(helper_bundle) = app.bundleIdentifier() {
            if helper_bundle.to_string().starts_with(bundle_id) {
                pids.insert(app.processIdentifier());
            }
        }
    }

    pids
}

// ---------------------------------------------------------------------------
// CFDictionary helpers
// ---------------------------------------------------------------------------

/// # Safety
///
/// `dict` must be a valid `CFDictionaryRef`. `key` must be a valid `CFStringRef`.
/// Both must remain valid for the duration of this call.
pub unsafe fn get_dict_string(dict: CFDictionaryRef, key: CFStringRef) -> Option<String> {
    // SAFETY: FFI calls on valid CoreGraphics/CoreFoundation objects.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "multiple FFI calls")]
    unsafe {
        let val = CFDictionaryGetValue(dict, key.cast::<std::ffi::c_void>());
        if val.is_null() {
            return None;
        }
        // Check if it's actually a CFString
        if CFGetTypeID(val as CFTypeRef) != CFStringGetTypeID() {
            return None;
        }
        // CFStringGetCStringPtr only works for "null-fast" strings (pure ASCII/UTF-8).
        // For non-ASCII characters (emojis, CJK, etc.) it returns NULL. Use the
        // slower CFStringGetCString as a fallback, which handles all encodings.
        let ptr = CFStringGetCStringPtr(val as CFStringRef, K_CF_STRING_ENCODING_UTF8);
        if !ptr.is_null() {
            return std::ffi::CStr::from_ptr(ptr)
                .to_str()
                .ok()
                .map(String::from);
        }
        // Fallback: copy into a buffer
        let mut buf = [0_u8; 1024];
        #[expect(
            clippy::cast_possible_wrap,
            reason = "buffer length fits in CFIndex (i64)"
        )]
        let buf_len = buf.len() as CFIndex;
        if CFStringGetCString(
            val as CFStringRef,
            buf.as_mut_ptr().cast::<std::ffi::c_char>(),
            buf_len,
            K_CF_STRING_ENCODING_UTF8,
        ) {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            buf.get(..len)
                .and_then(|slice| std::str::from_utf8(slice).ok())
                .map(String::from)
        } else {
            None
        }
    }
}

unsafe fn get_dict_i32(dict: CFDictionaryRef, key: CFStringRef) -> Option<i32> {
    // SAFETY: FFI calls on valid CoreGraphics/CoreFoundation objects.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "multiple FFI calls")]
    unsafe {
        let val = CFDictionaryGetValue(dict, key.cast::<std::ffi::c_void>());
        if val.is_null() {
            return None;
        }
        if CFGetTypeID(val as CFTypeRef) != CFNumberGetTypeID() {
            return None;
        }
        let mut result: i32 = 0;
        if CFNumberGetValue(
            val as CFNumberRef,
            K_CF_NUMBER_SINT32_TYPE,
            (&raw mut result).cast::<std::ffi::c_void>(),
        ) != 0
        {
            Some(result)
        } else {
            None
        }
    }
}

unsafe fn get_dict_bounds(dict: CFDictionaryRef, key: CFStringRef) -> Option<Rect> {
    // SAFETY: FFI calls on valid CoreGraphics/CoreFoundation objects.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "multiple FFI calls")]
    unsafe {
        let val = CFDictionaryGetValue(dict, key.cast::<std::ffi::c_void>());
        if val.is_null() {
            return None;
        }
        // The bounds value is a CFDictionary with X, Y, Width, Height keys
        if CFGetTypeID(val as CFTypeRef) != CFDictionaryGetTypeID() {
            return None;
        }
        let bounds_dict = val as CFDictionaryRef;
        let x = get_dict_f64_local(bounds_dict, "X").unwrap_or(0.0);
        let y = get_dict_f64_local(bounds_dict, "Y").unwrap_or(0.0);
        let w = get_dict_f64_local(bounds_dict, "Width").unwrap_or(0.0);
        let h = get_dict_f64_local(bounds_dict, "Height").unwrap_or(0.0);
        Some(Rect {
            x,
            y,
            width: w,
            height: h,
        })
    }
}

/// Local helper to get an f64 from a `CFDictionary` with a string key.
unsafe fn get_dict_f64_local(dict: CFDictionaryRef, key: &str) -> Option<f64> {
    // SAFETY: FFI calls on valid CoreGraphics/CoreFoundation objects.
    #[expect(clippy::multiple_unsafe_ops_per_block, reason = "multiple FFI calls")]
    unsafe {
        let cf_key = cf_string_from_str(key);
        let val = CFDictionaryGetValue(dict, cf_key.cast::<std::ffi::c_void>());
        CFRelease(cf_key as CFTypeRef);
        if val.is_null() {
            return None;
        }
        if CFGetTypeID(val as CFTypeRef) != CFNumberGetTypeID() {
            return None;
        }
        let mut result: f64 = 0.0;
        if CFNumberGetValue(
            val as CFNumberRef,
            K_CF_NUMBER_DOUBLE_TYPE,
            (&raw mut result).cast::<std::ffi::c_void>(),
        ) != 0
        {
            Some(result)
        } else {
            None
        }
    }
}

/// Create a `CFString` from a Rust &str.
///
/// The returned `CFStringRef` is a new reference that the caller must
/// release with `CFRelease`. `NSString` is toll-free bridged with `CFString`,
/// so we can create an `NSString` and cast its pointer.
#[must_use]
pub fn cf_string_from_str(s: &str) -> CFStringRef {
    // NSString::from_str creates an autoreleased string.
    // We retain it manually so the caller owns it.
    let ns = NSString::from_str(s);
    let ptr = Retained::as_ptr(&ns) as CFStringRef;
    // Prevent Drop from releasing -- caller takes ownership via CFRelease
    #[expect(
        clippy::mem_forget,
        reason = "transfer ownership to caller via CFRelease"
    )]
    std::mem::forget(ns);
    ptr
}

// ---------------------------------------------------------------------------
// Electron tree check
// ---------------------------------------------------------------------------

fn has_populated_web_area(element: AXUIElementRef, depth: usize, max_depth: usize) -> bool {
    if depth >= max_depth {
        return false;
    }

    // Use batched attribute fetching instead of individual `get_ax_string` calls.
    // Individual AX calls can return errors for children of Electron apps during
    // tree building, but `AXUIElementCopyMultipleAttributeValues` handles them correctly.
    let Some(attrs) = fetch_batch_attributes(element) else {
        return false;
    };

    let role = attrs.string(ATTR_ROLE);

    if role.as_deref() == Some("AXWebArea") {
        let children = attrs.children(ATTR_CHILDREN);
        return !children.is_empty();
    }

    let children = attrs.children(ATTR_CHILDREN);
    for child in &children {
        if has_populated_web_area(*child, depth + 1, max_depth) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: u32, title: &str, w: f64, h: f64) -> WindowEntry {
        WindowEntry {
            id,
            title: title.to_string(),
            bounds: Rect::new(0.0, 0.0, w, h),
        }
    }

    // --- ResolvedWindow ---

    #[test]
    fn resolved_window_origin() {
        let rw = ResolvedWindow {
            window_id: 42,
            title: "Test".to_string(),
            bounds: Rect::new(100.0, 200.0, 800.0, 600.0),
        };
        assert_eq!(rw.origin(), Point::new(100.0, 200.0));
    }

    #[test]
    fn resolved_window_center() {
        let rw = ResolvedWindow {
            window_id: 42,
            title: "Test".to_string(),
            bounds: Rect::new(100.0, 200.0, 800.0, 600.0),
        };
        assert_eq!(rw.center(), Point::new(500.0, 500.0));
    }

    // --- match_window ---

    #[test]
    fn window_id_match() {
        let windows = vec![
            make_entry(100, "Document", 800.0, 600.0),
            make_entry(200, "Settings", 400.0, 300.0),
        ];
        let result = match_window_by_id(&windows, "200").unwrap();
        assert_eq!(result.window_id, 200);
        assert_eq!(result.title, "Settings");
    }

    #[test]
    fn window_id_not_found() {
        let windows = vec![make_entry(100, "Doc", 800.0, 600.0)];
        let err = match_window_by_id(&windows, "999").unwrap_err();
        match err {
            ForepawError::WindowNotFound(q) => assert_eq!(q, "999"),
            other => panic!("expected WindowNotFound, got {other:?}"),
        }
    }

    #[test]
    fn window_title_substring_match() {
        let windows = vec![
            make_entry(100, "My Document.txt", 800.0, 600.0),
            make_entry(200, "Settings", 400.0, 300.0),
        ];
        let result = match_window_by_title(&windows, "document").unwrap();
        assert_eq!(result.window_id, 100);
    }

    #[test]
    fn window_title_case_insensitive() {
        let windows = vec![make_entry(100, "Document", 800.0, 600.0)];
        let result = match_window_by_title(&windows, "DOCUMENT").unwrap();
        assert_eq!(result.window_id, 100);
    }

    #[test]
    fn window_title_ambiguous() {
        let windows = vec![
            make_entry(100, "Document 1", 800.0, 600.0),
            make_entry(200, "Document 2", 800.0, 600.0),
        ];
        let err = match_window_by_title(&windows, "Document").unwrap_err();
        match err {
            ForepawError::AmbiguousWindow { query, matches: _ } => {
                assert_eq!(query, "Document");
            }
            other => panic!("expected AmbiguousWindow, got {other:?}"),
        }
    }

    #[test]
    fn window_title_no_match() {
        let windows = vec![make_entry(100, "Doc", 800.0, 600.0)];
        let err = match_window_by_title(&windows, "nonexistent").unwrap_err();
        match err {
            ForepawError::WindowNotFound(q) => assert_eq!(q, "nonexistent"),
            other => panic!("expected WindowNotFound, got {other:?}"),
        }
    }

    // --- select_best_window ---

    #[test]
    fn select_best_prefers_titled() {
        let windows = vec![
            make_entry(100, "", 1000.0, 800.0),
            make_entry(200, "Document", 400.0, 300.0),
        ];
        let result = select_best_window(&windows);
        assert_eq!(result.window_id, 200);
        assert_eq!(result.title, "Document");
    }

    #[test]
    fn select_best_largest_area_as_tiebreak() {
        let windows = vec![
            make_entry(100, "Small", 400.0, 300.0),
            make_entry(200, "Large", 800.0, 600.0),
        ];
        let result = select_best_window(&windows);
        assert_eq!(result.window_id, 200);
    }

    #[test]
    fn select_best_untitled_largest_area() {
        let windows = vec![
            make_entry(100, "", 800.0, 600.0),
            make_entry(200, "", 400.0, 300.0),
        ];
        let result = select_best_window(&windows);
        assert_eq!(result.window_id, 100);
    }

    #[test]
    fn select_best_single_window() {
        let windows = vec![make_entry(100, "Only", 800.0, 600.0)];
        let result = select_best_window(&windows);
        assert_eq!(result.window_id, 100);
    }
}
