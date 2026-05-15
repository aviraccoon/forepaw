//! Application and window enumeration on Windows.
//!
//! Uses EnumWindows to enumerate top-level windows, then extracts process
//! info (name, PID) and window info (title, bounds, window handle).

use std::collections::HashMap;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, SetForegroundWindow,
};

use crate::core::errors::ForepawError;
use crate::core::types::Rect;
use crate::platform::{AppInfo, WindowInfo};

/// List running GUI applications.
///
/// Enumerates all visible top-level windows, deduplicates by owning process,
/// and returns one AppInfo per process. Process name comes from the executable
/// filename (without extension), which matches how Windows users identify apps.
///
/// Exception: UWP apps run inside `ApplicationFrameHost.exe`. Multiple UWP apps
/// can share the same host process but have separate windows with distinct titles
/// (e.g. "Calculator", "Settings"). For these, we emit one AppInfo per window
/// so each app appears separately in the listing.
pub fn list_apps() -> Result<Vec<AppInfo>, ForepawError> {
    let windows = collect_visible_windows();

    // Group by PID, pick one window per process (first with a title)
    let mut by_pid: HashMap<u32, Vec<WindowEntry>> = HashMap::new();
    for entry in windows {
        by_pid.entry(entry.pid).or_default().push(entry);
    }

    let mut apps = Vec::new();
    for (pid, entries) in by_pid {
        let process_name = get_process_name(pid).unwrap_or_else(|| format!("pid-{pid}"));

        // UWP apps (ApplicationFrameHost) get one entry per titled window.
        // Multiple UWP apps can share the same host process.
        if process_name.eq_ignore_ascii_case("ApplicationFrameHost") {
            for entry in &entries {
                if !entry.title.is_empty() {
                    apps.push(AppInfo {
                        name: entry.title.clone(),
                        // Use the window title as bundle_id for UWP apps
                        // (the actual exe name is unhelpful)
                        bundle_id: Some(entry.title.clone()),
                        pid: pid as i32,
                    });
                }
            }
            continue;
        }

        // Regular apps: one entry per process
        let display_name = entries
            .iter()
            .find(|e| !e.title.is_empty())
            .map_or_else(|| process_name.clone(), |e| e.title.clone());

        apps.push(AppInfo {
            name: display_name,
            // Use executable name as "bundle ID" on Windows (no real bundle IDs)
            bundle_id: Some(process_name),
            pid: pid as i32,
        });
    }

    apps.sort_by_key(|a| a.name.to_lowercase());
    Ok(apps)
}

/// List visible windows, optionally filtered by app name.
///
/// App name matching checks both the window title and the owning process
/// executable name (case-insensitive substring match).
pub fn list_windows(app_name: Option<&str>) -> Result<Vec<WindowInfo>, ForepawError> {
    let entries = collect_visible_windows();

    let filtered: Vec<WindowEntry> = match app_name {
        Some(filter) => {
            let filter_lower = filter.to_lowercase();
            entries
                .into_iter()
                .filter(|e| {
                    // Match against window title
                    e.title.to_lowercase().contains(&filter_lower)
                        // Or against process executable name
                        || e.process_name.to_lowercase().contains(&filter_lower)
                })
                .collect()
        }
        None => entries,
    };

    Ok(filtered.into_iter().map(Into::into).collect())
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct WindowEntry {
    hwnd: isize,
    pid: u32,
    title: String,
    bounds: Option<Rect>,
    process_name: String,
}

impl From<WindowEntry> for WindowInfo {
    fn from(e: WindowEntry) -> Self {
        WindowInfo {
            id: format!("w-{}", e.hwnd),
            title: e.title,
            app: e.process_name,
            bounds: e.bounds,
        }
    }
}

// ---------------------------------------------------------------------------
// Window enumeration
// ---------------------------------------------------------------------------

fn collect_visible_windows() -> Vec<WindowEntry> {
    let mut entries: Vec<WindowEntry> = Vec::new();

    unsafe {
        let _ = EnumWindows(
            Some(enum_window_callback),
            LPARAM(&mut entries as *mut Vec<WindowEntry> as isize),
        );
    }

    entries
}

unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let entries = &mut *(lparam.0 as *mut Vec<WindowEntry>);

    // Skip invisible windows
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1); // continue enumeration
    }

    // Get window title
    let title = get_window_text(hwnd);

    // Get owning process
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };

    let process_name = get_process_name(pid).unwrap_or_else(|| format!("pid-{pid}"));

    // Get window bounds
    let mut rect = RECT::default();
    let bounds = if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
        let r = Rect::new(
            f64::from(rect.left),
            f64::from(rect.top),
            f64::from(rect.right - rect.left),
            f64::from(rect.bottom - rect.top),
        );
        // Skip tiny/phantom windows (same filter as macOS backend)
        if r.width < 10.0 || r.height < 10.0 {
            None
        } else {
            Some(r)
        }
    } else {
        None
    };

    // Skip windows without bounds (phantom windows)
    let Some(bounds) = bounds else {
        return BOOL(1);
    };

    entries.push(WindowEntry {
        hwnd: hwnd.0 as isize,
        pid,
        title,
        bounds: Some(bounds),
        process_name,
    });

    BOOL(1) // continue enumeration
}

/// Find the best visible window matching an app name (case-insensitive substring).
///
/// Matches against both window title and process executable name.
/// Returns the window handle and its bounds.
///
/// Selection priority:
/// 1. Windows whose title matches the query (user explicitly targets by name)
/// 2. Non-desktop windows with a title (avoid "Program Manager" shell window)
/// 3. Largest window by area as tiebreaker
pub fn find_app_hwnd(app_name: &str) -> Result<(HWND, Rect), ForepawError> {
    let entries = collect_visible_windows();
    let filter_lower = app_name.to_lowercase();

    let matching: Vec<&WindowEntry> = entries
        .iter()
        .filter(|e| {
            e.title.to_lowercase().contains(&filter_lower)
                || e.process_name.to_lowercase().contains(&filter_lower)
        })
        .collect();

    if matching.is_empty() {
        return Err(ForepawError::AppNotFound(app_name.to_string()));
    }

    // Score each candidate: prefer title match > non-desktop > titled > largest area
    let best = matching
        .iter()
        .max_by_key(|e| {
            let title_match = e.title.to_lowercase().contains(&filter_lower);
            let is_desktop = e.title == "Program Manager";
            let has_title = !e.title.is_empty();
            let area = e.bounds.as_ref().map_or(0, |b| (b.width * b.height) as u64);

            // Pack into a tuple for lexicographic comparison:
            // (title_matches_query, not_desktop, has_title, area)
            (title_match, !is_desktop, has_title, area)
        })
        .unwrap();

    let bounds = best
        .bounds
        .ok_or_else(|| ForepawError::ActionFailed("matched window has no bounds".into()))?;

    Ok((HWND(best.hwnd as *mut std::ffi::c_void), bounds))
}

/// Bring a window to the foreground.
pub fn activate_app(hwnd: HWND) {
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get window title text via GetWindowTextLengthW + GetWindowTextW.
fn get_window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0_u16; (len as usize) + 1];
        let written = GetWindowTextW(hwnd, &mut buf);
        if written == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..written as usize])
    }
}

/// Get the process executable name (without path or extension) for a PID.
fn get_process_name(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        // QueryFullProcessImageNameW returns the full path.
        let mut size: u32 = 1024;
        let mut buf = vec![0_u16; size as usize];
        let flags = PROCESS_NAME_WIN32;
        QueryFullProcessImageNameW(
            process,
            flags,
            windows::core::PWSTR::from_raw(buf.as_mut_ptr()),
            &mut size,
        )
        .ok()?;
        let full_path = String::from_utf16_lossy(&buf[..size as usize]);

        // Extract filename without extension
        let filename = full_path.rsplit('\\').next().unwrap_or(&full_path);
        let name = filename.strip_suffix(".exe").unwrap_or(filename);
        Some(name.to_string())
    }
}
