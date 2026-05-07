//! Application and window lookup via NSWorkspace and CGWindowList.
//!
//! Provides the foundational lookup functions used by snapshot, input, and
//! screenshot modules. All functions return platform-agnostic types from
//! `crate::platform` or `crate::core::types`.

use std::collections::HashSet;
use std::path::Path;

use objc2::Message;
use objc2::rc::Retained;
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

use crate::core::element_tree::is_interactive_role;
use crate::core::errors::ForepawError;
use crate::core::types::{Point, Rect};
use crate::platform::darwin::ffi;
use crate::platform::darwin::ffi::*;
use crate::platform::{AppInfo, WindowInfo};

// ---------------------------------------------------------------------------
// Permission gating
// ---------------------------------------------------------------------------

/// Check accessibility permission. Returns error if not granted.
fn require_accessibility() -> Result<(), ForepawError> {
    if unsafe { ffi::AXIsProcessTrusted() } == 0 {
        Err(ForepawError::PermissionDenied)
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Application lookup
// ---------------------------------------------------------------------------

/// Find a running application by exact name, bundle ID, or case-insensitive
/// partial match (in that priority order).
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

    Err(ForepawError::AppNotFound(name.to_string()))
}

/// List all running applications with regular activation policy.
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
        result.push(AppInfo {
            name,
            bundle_id,
            pid,
        });
    }

    Ok(result)
}
// ---------------------------------------------------------------------------

/// A resolved window with its CGWindowID, title, and bounds.
pub struct ResolvedWindow {
    pub window_id: u32,
    pub title: String,
    pub bounds: Rect,
}

impl ResolvedWindow {
    pub fn origin(&self) -> Point {
        Point {
            x: self.bounds.x,
            y: self.bounds.y,
        }
    }

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
pub fn find_window(pid: i32, window: Option<&str>) -> Result<ResolvedWindow, ForepawError> {
    require_accessibility()?;
    let window_list = unsafe {
        CGWindowListCopyWindowInfo(CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY, K_CG_NULL_WINDOW_ID)
    };
    if window_list.is_null() {
        return Err(ForepawError::WindowNotFound(
            window.unwrap_or("any").to_string(),
        ));
    }

    let mut app_windows = unsafe { collect_windows_for_pid(window_list, pid) };

    // Multi-process fallback: some apps (Steam) render UI in helper processes.
    if app_windows.is_empty() {
        if let Ok(main_app) = find_app_by_pid(pid) {
            if let Some(main_bundle) = main_app.bundleIdentifier() {
                let main_bundle_str = main_bundle.to_string();
                let helper_pids = collect_helper_pids(&main_bundle_str, pid);
                if !helper_pids.is_empty() {
                    app_windows = unsafe { collect_windows_for_pids(window_list, &helper_pids) };
                }
            }
        }
    }

    unsafe { CFRelease(window_list as CFTypeRef) };

    if app_windows.is_empty() {
        return Err(ForepawError::WindowNotFound(
            window.unwrap_or("any").to_string(),
        ));
    }

    match window {
        Some(w) => match_window(&app_windows, w),
        None => select_best_window(&app_windows),
    }
}

/// List visible windows, optionally filtered by app name.
///
/// Returns an error if accessibility permission is not granted, since
/// the window list would be incomplete without it.
pub fn list_windows(app_name: Option<&str>) -> Result<Vec<WindowInfo>, ForepawError> {
    let window_list = unsafe {
        CGWindowListCopyWindowInfo(CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY, K_CG_NULL_WINDOW_ID)
    };
    if window_list.is_null() {
        return Ok(Vec::new());
    }

    require_accessibility()?;

    // Build set of allowed PIDs if filtering by app name
    let allowed_pids: Option<HashSet<i32>> = app_name.map(|name| {
        match find_app(name) {
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
        }
    });

    let count = unsafe { CFArrayGetCount(window_list) };
    let mut result = Vec::new();

    for i in 0..count {
        let info = unsafe { CFArrayGetValueAtIndex(window_list, i as _) as CFDictionaryRef };
        if info.is_null() {
            continue;
        }

        // Filter by PID set if applicable
        if let Some(ref pids) = allowed_pids {
            let owner_pid = unsafe { get_dict_i32(info, kCGWindowOwnerPID) };
            match owner_pid {
                Some(pid) if pids.contains(&pid) => {}
                _ => continue,
            }
        }

        let owner_name = unsafe { get_dict_string(info, kCGWindowOwnerName) };
        let window_id = unsafe { get_dict_i32(info, kCGWindowNumber) };
        let title =
            unsafe { get_dict_string(info, kCGWindowName) }.unwrap_or_default();

        let (owner, id_num) = match (owner_name, window_id) {
            (Some(o), Some(id)) => (o, id),
            _ => continue,
        };

        // Filter by app name if no PID set was built (find_app failed)
        if allowed_pids.is_none() {
            if let Some(filter) = app_name {
                if owner != filter {
                    continue;
                }
            }
        }

        // Skip phantom/tiny windows
        if let Some(bounds) = unsafe { get_dict_bounds(info, kCGWindowBounds) } {
            if bounds.width < 10.0 || bounds.height < 10.0 {
                continue;
            }
            result.push(WindowInfo {
                id: format!("w-{id_num}"),
                title,
                app: owner,
                bounds: Some(bounds),
            });
        }
    }

    unsafe { CFRelease(window_list as CFTypeRef) };
    Ok(result)
}

// ---------------------------------------------------------------------------
// Coordinate conversion
// ---------------------------------------------------------------------------

/// Convert window-relative coordinates to screen-absolute coordinates.
pub fn to_screen_point(point: &Point, pid: i32) -> Result<Point, ForepawError> {
    let resolved = find_window(pid, None)?;
    Ok(Point {
        x: point.x + resolved.bounds.x,
        y: point.y + resolved.bounds.y,
    })
}

/// Validate that a point is within the window bounds.
pub fn validate_point_in_window(point: &Point, pid: i32) -> Result<(), ForepawError> {
    let resolved = find_window(pid, None)?;
    let w = resolved.bounds.width;
    let h = resolved.bounds.height;
    if point.x < 0.0 || point.y < 0.0 || point.x > w || point.y > h {
        return Err(ForepawError::ActionFailed(format!(
            "Point ({}, {}) is outside window bounds (0,0)-({},{})",
            point.x as i32,
            point.y as i32,
            w as i32,
            h as i32
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Electron detection
// ---------------------------------------------------------------------------

/// Check if an app bundle contains the Electron Framework.
/// CEF apps like Spotify are NOT included -- CEF doesn't respond to
/// AXManualAccessibility and exposes only empty group nodes.
pub fn is_electron_app(app: &NSRunningApplication) -> bool {
    let bundle_url = match app.bundleURL() {
        Some(url) => url,
        None => return false,
    };
    let path = bundle_url.path().unwrap_or_default();
    let framework_path = format!("{path}/Contents/Frameworks/Electron Framework.framework");
    Path::new(&framework_path).exists()
}

/// Tell an Electron app to build its Chromium accessibility tree.
/// Sets the `AXManualAccessibility` attribute on the app element.
pub fn enable_electron_accessibility(pid: i32) {
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    let attr_name = cf_string_from_str("AXManualAccessibility");
    let val = objc2_foundation::NSNumber::numberWithBool(true);
    let cf_val = Retained::as_ptr(&val) as CFTypeRef;
    // Prevent Drop -- the AX API doesn't retain it, but we need it alive
    // through the AXUIElementSetAttributeValue call.
    std::mem::forget(val);
    unsafe {
        AXUIElementSetAttributeValue(app_element, attr_name, cf_val);
        CFRelease(attr_name as CFTypeRef);
    }
}

/// Check if an Electron app's web content tree is populated.
/// Looks for an AXWebArea with interactive children.
pub fn electron_tree_is_populated(pid: i32) -> bool {
    let app_element = unsafe { AXUIElementCreateApplication(pid) };
    has_populated_web_area(app_element, 0, 10)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

struct WindowEntry {
    id: u32,
    title: String,
    bounds: Rect,
}

unsafe fn collect_windows_for_pid(window_list: CFArrayRef, pid: i32) -> Vec<WindowEntry> {
    let count = unsafe { CFArrayGetCount(window_list) };
    let mut entries = Vec::new();

    for i in 0..count {
        let info = unsafe { CFArrayGetValueAtIndex(window_list, i as _) as CFDictionaryRef };
        if info.is_null() {
            continue;
        }
        let _owner_pid = match get_dict_i32(info, unsafe { kCGWindowOwnerPID }) {
            Some(p) if p == pid => p,
            _ => continue,
        };
        let bounds_key = unsafe { kCGWindowBounds };
        let bounds = match get_dict_bounds(info, bounds_key) {
            Some(b) if b.width >= 10.0 && b.height >= 10.0 => b,
            _ => continue,
        };
        let window_id = get_dict_i32(info, unsafe { kCGWindowNumber })
            .and_then(|id| if id > 0 { Some(id as u32) } else { None })
            .unwrap_or(0);
        let title = get_dict_string(info, unsafe { kCGWindowName }).unwrap_or_default();

        entries.push(WindowEntry {
            id: window_id,
            title,
            bounds,
        });
    }

    entries
}

unsafe fn collect_windows_for_pids(window_list: CFArrayRef, pids: &HashSet<i32>) -> Vec<WindowEntry> {
    let count = unsafe { CFArrayGetCount(window_list) };
    let mut entries = Vec::new();

    for i in 0..count {
        let info = unsafe { CFArrayGetValueAtIndex(window_list, i as _) as CFDictionaryRef };
        if info.is_null() {
            continue;
        }
        let _owner_pid = match get_dict_i32(info, unsafe { kCGWindowOwnerPID }) {
            Some(p) if pids.contains(&p) => p,
            _ => continue,
        };
        let bounds_key = unsafe { kCGWindowBounds };
        let bounds = match get_dict_bounds(info, bounds_key) {
            Some(b) if b.width >= 10.0 && b.height >= 10.0 => b,
            _ => continue,
        };
        let window_id = get_dict_i32(info, unsafe { kCGWindowNumber })
            .and_then(|id| if id > 0 { Some(id as u32) } else { None })
            .unwrap_or(0);
        let title = get_dict_string(info, unsafe { kCGWindowName }).unwrap_or_default();

        entries.push(WindowEntry {
            id: window_id,
            title,
            bounds,
        });
    }

    entries
}

fn match_window(windows: &[WindowEntry], pattern: &str) -> Result<ResolvedWindow, ForepawError> {
    // Match by window ID: "w-1234"
    if let Some(id_str) = pattern.strip_prefix("w-") {
        if let Ok(id_num) = id_str.parse::<u32>() {
            if let Some(w) = windows.iter().find(|w| w.id == id_num) {
                return Ok(ResolvedWindow {
                    window_id: w.id,
                    title: w.title.clone(),
                    bounds: w.bounds,
                });
            }
        }
        return Err(ForepawError::WindowNotFound(pattern.to_string()));
    }

    // Substring match on title (case-insensitive)
    let pattern_lower = pattern.to_lowercase();
    let matches: Vec<&WindowEntry> = windows
        .iter()
        .filter(|w| w.title.to_lowercase().contains(&pattern_lower))
        .collect();

    match matches.len() {
        1 => {
            let m = matches[0];
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
                query: pattern.to_string(),
                matches: titles,
            })
        }
        0 => Err(ForepawError::WindowNotFound(pattern.to_string())),
    }
}

fn select_best_window(windows: &[WindowEntry]) -> Result<ResolvedWindow, ForepawError> {
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
        .unwrap(); // safe: windows is non-empty

    Ok(ResolvedWindow {
        window_id: best.id,
        title: best.title.clone(),
        bounds: best.bounds,
    })
}

fn find_app_by_pid(pid: i32) -> Result<Retained<NSRunningApplication>, ForepawError> {
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
/// `dict` must be a valid CFDictionaryRef. `key` must be a valid CFStringRef.
/// Both must remain valid for the duration of this call.
pub unsafe fn get_dict_string(dict: CFDictionaryRef, key: CFStringRef) -> Option<String> {
    unsafe {
        let val = CFDictionaryGetValue(dict, key as *const std::ffi::c_void);
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
            return std::ffi::CStr::from_ptr(ptr).to_str().ok().map(String::from);
        }
        // Fallback: copy into a buffer
        let mut buf = [0u8; 1024];
        if CFStringGetCString(
            val as CFStringRef,
            buf.as_mut_ptr() as *mut std::ffi::c_char,
            buf.len() as CFIndex,
            K_CF_STRING_ENCODING_UTF8,
        ) {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            std::str::from_utf8(&buf[..len]).ok().map(String::from)
        } else {
            None
        }
    }
}

unsafe fn get_dict_i32(dict: CFDictionaryRef, key: CFStringRef) -> Option<i32> {
    unsafe {
        let val = CFDictionaryGetValue(dict, key as *const std::ffi::c_void);
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
            &mut result as *mut i32 as *mut std::ffi::c_void,
        ) != 0
        {
            Some(result)
        } else {
            None
        }
    }
}

unsafe fn get_dict_bounds(dict: CFDictionaryRef, key: CFStringRef) -> Option<Rect> {
    unsafe {
        let val = CFDictionaryGetValue(dict, key as *const std::ffi::c_void);
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

/// Local helper to get an f64 from a CFDictionary with a string key.
unsafe fn get_dict_f64_local(dict: CFDictionaryRef, key: &str) -> Option<f64> {
    unsafe {
        let cf_key = cf_string_from_str(key);
        let val = CFDictionaryGetValue(dict, cf_key as *const std::ffi::c_void);
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
            &mut result as *mut f64 as *mut std::ffi::c_void,
        ) != 0
        {
            Some(result)
        } else {
            None
        }
    }
}

/// Create a CFString from a Rust &str.
///
/// The returned CFStringRef is a new reference that the caller must
/// release with CFRelease. NSString is toll-free bridged with CFString,
/// so we can create an NSString and cast its pointer.
pub fn cf_string_from_str(s: &str) -> CFStringRef {
    // NSString::from_str creates an autoreleased string.
    // We retain it manually so the caller owns it.
    let ns = NSString::from_str(s);
    let ptr = Retained::as_ptr(&ns) as CFStringRef;
    // Prevent Drop from releasing -- caller takes ownership via CFRelease
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

    let role = get_ax_string(element, "AXRole");

    if role.as_deref() == Some("AXWebArea") {
        // Check for interactive children
        let children = get_ax_children(element);
        for child in &children {
            let child_role = get_ax_string(*child, "AXRole").unwrap_or_default();
            if is_interactive_role(&child_role) {
                return true;
            }
            // Check one level deeper for interactive content inside groups
            let grandchildren = get_ax_children(*child);
            for gc in &grandchildren {
                let gc_role = get_ax_string(*gc, "AXRole").unwrap_or_default();
                if is_interactive_role(&gc_role) {
                    return true;
                }
            }
        }
        return false;
    }

    let children = get_ax_children(element);
    for child in &children {
        if has_populated_web_area(*child, depth + 1, max_depth) {
            return true;
        }
    }

    false
}

/// Get a string attribute from an AXUIElement.
fn get_ax_string(element: AXUIElementRef, attribute: &str) -> Option<String> {
    unsafe {
        let attr_cf = cf_string_from_str(attribute);
        let mut value: CFTypeRef = std::ptr::null();
        let result = AXUIElementCopyAttributeValue(element, attr_cf, &mut value);
        CFRelease(attr_cf as CFTypeRef);
        if result != AXError::Success || value.is_null() {
            return None;
        }
        if CFGetTypeID(value) != CFStringGetTypeID() {
            CFRelease(value);
            return None;
        }
        let ptr = CFStringGetCStringPtr(value as CFStringRef, K_CF_STRING_ENCODING_UTF8);
        let s = if ptr.is_null() {
            None
        } else {
            std::ffi::CStr::from_ptr(ptr)
                .to_str()
                .ok()
                .map(String::from)
        };
        CFRelease(value);
        s
    }
}

/// Get the AXChildren attribute as a Vec of AXUIElementRef.
fn get_ax_children(element: AXUIElementRef) -> Vec<AXUIElementRef> {
    unsafe {
        let attr_cf = cf_string_from_str("AXChildren");
        let mut value: CFTypeRef = std::ptr::null();
        let result = AXUIElementCopyAttributeValue(element, attr_cf, &mut value);
        CFRelease(attr_cf as CFTypeRef);
        if result != AXError::Success || value.is_null() {
            return Vec::new();
        }
        if CFGetTypeID(value) != CFArrayGetTypeID() {
            CFRelease(value);
            return Vec::new();
        }
        let count = CFArrayGetCount(value as CFArrayRef);
        let mut children = Vec::with_capacity(count as usize);
        for i in 0..count {
            let child = CFArrayGetValueAtIndex(value as CFArrayRef, i);
            // AXUIElementRef is a newtype around *const c_void, so we need
            // to transmute the raw pointer.
            children.push(AXUIElementRef::from_raw(child));
        }
        CFRelease(value);
        children
    }
}
