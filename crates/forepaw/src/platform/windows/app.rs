//! Application and window enumeration on Windows.
//!
//! Uses `EnumWindows` to enumerate top-level windows, then extracts process
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
use crate::platform::AppTarget;
use crate::platform::{AppInfo, WindowInfo, WindowTarget};

/// List running GUI applications.
///
/// Enumerates all visible top-level windows, deduplicates by owning process,
/// and returns one `AppInfo` per process. Process name comes from the executable
/// filename (without extension), which matches how Windows users identify apps.
///
/// Exception: UWP apps run inside `ApplicationFrameHost.exe`. Multiple UWP apps
/// can share the same host process but have separate windows with distinct titles
/// (e.g. "Calculator", "Settings"). For these, we emit one `AppInfo` per window
/// so each app appears separately in the listing.
///
/// # Errors
///
/// Returns [`ForepawError::PermissionDenied`] if accessibility access is not granted.
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
                    #[expect(clippy::cast_possible_wrap, reason = "PID fits in i32")]
                    let pid_i32 = pid as i32;
                    apps.push(AppInfo {
                        name: entry.title.clone(),
                        // Use the window title as bundle_id for UWP apps
                        // (the actual exe name is unhelpful)
                        bundle_id: Some(entry.title.clone()),
                        pid: pid_i32,
                        // TODO: use GetForegroundWindow() + GetWindowThreadProcessId()
                        is_active: false,
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

        #[expect(clippy::cast_possible_wrap, reason = "PID fits in i32")]
        let pid_i32 = pid as i32;
        apps.push(AppInfo {
            name: display_name,
            // Use executable name as "bundle ID" on Windows (no real bundle IDs)
            bundle_id: Some(process_name),
            pid: pid_i32,
            // TODO: use GetForegroundWindow() + GetWindowThreadProcessId()
            is_active: false,
        });
    }

    apps.sort_by_key(|a| a.name.to_lowercase());
    Ok(apps)
}

/// List visible windows, optionally filtered by app name.
///
/// App name matching checks both the window title and the owning process
/// executable name (case-insensitive substring match).
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if `app` is provided but no matching window is found.
pub fn list_windows(app: Option<&AppTarget>) -> Result<Vec<WindowInfo>, ForepawError> {
    let entries = collect_visible_windows();

    let filtered: Vec<WindowEntry> = match app {
        Some(target) => match target {
            AppTarget::Name(name) => {
                let filter_lower = name.to_lowercase();
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
            AppTarget::Pid(pid) => {
                #[expect(clippy::cast_sign_loss, reason = "PID from system is positive")]
                let pid_u32 = *pid as u32;
                entries.into_iter().filter(|e| e.pid == pid_u32).collect()
            }
        },
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
        // TODO: populate state via IsIconic (Minimized), IsZoomed (Maximized),
        // and bounds comparison for fullscreen. Needs VM testing.
        Self {
            id: format!("w-{}", e.hwnd),
            title: e.title,
            app: e.process_name,
            bounds: e.bounds,
            state: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Window enumeration
// ---------------------------------------------------------------------------

fn collect_visible_windows() -> Vec<WindowEntry> {
    let mut entries: Vec<WindowEntry> = Vec::new();

    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        EnumWindows(
            Some(enum_window_callback),
            LPARAM(&raw mut entries as isize),
        )
        .ok()
        .unwrap_or_default();
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
    // SAFETY: Win32 PID query on valid HWND.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };

    let process_name = get_process_name(pid).unwrap_or_else(|| format!("pid-{pid}"));

    // Get window bounds
    let mut rect = RECT::default();
    // SAFETY: Win32 GetWindowRect on valid HWND.
    let bounds = if unsafe { GetWindowRect(hwnd, &raw mut rect) }.is_ok() {
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
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if no window matches the query.
pub fn find_app_hwnd(
    app: &AppTarget,
    window: Option<&WindowTarget>,
) -> Result<(HWND, Rect), ForepawError> {
    let entries = collect_visible_windows();

    let matching: Vec<&WindowEntry> = match app {
        AppTarget::Name(name) => {
            let filter_lower = name.to_lowercase();
            entries
                .iter()
                .filter(|e| {
                    e.title.to_lowercase().contains(&filter_lower)
                        || e.process_name.to_lowercase().contains(&filter_lower)
                })
                .collect()
        }
        AppTarget::Pid(pid) => {
            #[expect(clippy::cast_sign_loss, reason = "PID from system is positive")]
            let pid_u32 = *pid as u32;
            entries.iter().filter(|e| e.pid == pid_u32).collect()
        }
    };

    if matching.is_empty() {
        return Err(ForepawError::AppNotFound(app.display()));
    }

    // If a window target is specified, use it to narrow down the match
    let best: &WindowEntry = if let Some(target) = window {
        find_window(&matching, target)?
    } else {
        // Score each candidate: prefer title match > non-desktop > titled > largest area
        let scored = match app {
            AppTarget::Name(name) => {
                let filter_lower = name.to_lowercase();
                matching.iter().max_by_key(|e| {
                    let title_match = e.title.to_lowercase().contains(&filter_lower);
                    let is_desktop = e.title == "Program Manager";
                    let has_title = !e.title.is_empty();
                    let area = e.bounds.as_ref().map_or(0_u64, |b| {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "screen dimensions fit in u64"
                        )]
                        #[expect(clippy::cast_sign_loss, reason = "window area is always positive")]
                        let area = (b.width * b.height) as u64;
                        area
                    });
                    (title_match, !is_desktop, has_title, area)
                })
            }
            AppTarget::Pid(_) => {
                // PID match is unambiguous, just pick the best window
                matching.iter().max_by_key(|e| {
                    let is_desktop = e.title == "Program Manager";
                    let has_title = !e.title.is_empty();
                    let area = e.bounds.as_ref().map_or(0_u64, |b| {
                        #[expect(
                            clippy::cast_possible_truncation,
                            reason = "screen dimensions fit in u64"
                        )]
                        #[expect(clippy::cast_sign_loss, reason = "window area is always positive")]
                        let area = (b.width * b.height) as u64;
                        area
                    });
                    (!is_desktop, has_title, area)
                })
            }
        };
        scored.ok_or_else(|| ForepawError::AppNotFound(app.display()))?
    };

    let bounds = best
        .bounds
        .ok_or_else(|| ForepawError::ActionFailed("matched window has no bounds".into()))?;

    Ok((HWND(best.hwnd as *mut std::ffi::c_void), bounds))
}

/// Bring a window to the foreground.
pub fn activate_app(hwnd: HWND) {
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        SetForegroundWindow(hwnd).ok().unwrap_or_default();
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
}

/// Find a window within an app's windows by ID (exact HWND match).
///
/// The ID string is the raw numeric HWND value (e.g. "131238" from `list-windows` w-131238).
///
/// # Errors
///
/// Returns [`ForepawError::WindowNotFound`] if no window matches.
fn match_window_by_id<'a>(
    windows: &[&'a WindowEntry],
    id: &str,
) -> Result<&'a WindowEntry, ForepawError> {
    // Validate as numeric (HWND is an isize pointer value)
    if let Ok(target_hwnd) = id.parse::<isize>() {
        if let Some(&w) = windows.iter().find(|w| w.hwnd == target_hwnd) {
            return Ok(w);
        }
    }
    Err(ForepawError::WindowNotFound(id.to_owned()))
}

/// Find a window within an app's windows by title substring.
///
/// Case-insensitive substring match. Returns `AmbiguousWindow` if multiple match.
///
/// # Errors
///
/// Returns [`ForepawError::WindowNotFound`] if no window matches,
/// or [`ForepawError::AmbiguousWindow`] if multiple windows match.
fn match_window_by_title<'a>(
    windows: &[&'a WindowEntry],
    pattern: &str,
) -> Result<&'a WindowEntry, ForepawError> {
    let pattern_lower = pattern.to_lowercase();
    let matches: Vec<&WindowEntry> = windows
        .iter()
        .copied()
        .filter(|w| w.title.to_lowercase().contains(&pattern_lower))
        .collect();

    match matches.len() {
        1 => Ok(matches.into_iter().next().expect("len == 1 checked")),
        2.. => {
            let titles = matches
                .iter()
                .map(|m| format!("  w-{}  {}", m.hwnd, m.title))
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

/// Find a specific window within an app's windows using a window target.
///
/// # Errors
///
/// Returns [`ForepawError::WindowNotFound`] or [`ForepawError::AmbiguousWindow`]
/// from the underlying match functions.
fn find_window<'a>(
    windows: &[&'a WindowEntry],
    target: &WindowTarget,
) -> Result<&'a WindowEntry, ForepawError> {
    match target {
        WindowTarget::Id(id) => match_window_by_id(windows, id),
        WindowTarget::Title(title) => match_window_by_title(windows, title),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get window title text via `GetWindowTextLengthW` + `GetWindowTextW`.
fn get_window_text(hwnd: HWND) -> String {
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "Win32/WinRT FFI pipeline"
    )]
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return String::new();
        }
        #[expect(
            clippy::cast_sign_loss,
            reason = "GetWindowTextLengthW returns non-negative"
        )]
        let buf_len = (len as usize) + 1;
        let mut buf = vec![0_u16; buf_len];
        let written = GetWindowTextW(hwnd, &mut buf);
        if written == 0 {
            return String::new();
        }
        #[expect(
            clippy::cast_sign_loss,
            reason = "GetWindowTextW returns non-negative character count"
        )]
        let end = written as usize;
        String::from_utf16_lossy(buf.get(..end).unwrap_or_default())
    }
}

/// Get the process executable name (without path or extension) for a PID.
fn get_process_name(pid: u32) -> Option<String> {
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "Win32/WinRT FFI pipeline"
    )]
    // SAFETY: Win32/WinRT FFI call with valid arguments.
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
            &raw mut size,
        )
        .ok()?;
        let full_path = String::from_utf16_lossy(buf.get(..size as usize).unwrap_or_default());

        // Extract filename without extension
        let filename = full_path.rsplit('\\').next().unwrap_or(&full_path);
        let name = filename.strip_suffix(".exe").unwrap_or(filename);
        Some(name.to_owned())
    }
}
