//! Physical display enumeration via Win32 `EnumDisplayMonitors`.
//!
//! Provides [`DisplayInfo`] for each monitor: logical bounds, DPI-derived scale
//! factor, device name, and primary flag. All bounds are in the global logical
//! coordinate space (DPI-aware, same space as window and element bounds under
//! `PER_MONITOR_AWARE_V2`).

use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateDCW, DeleteDC, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, DEVMODEW,
    ENUM_CURRENT_SETTINGS, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::ColorSystem::GetICMProfileW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MONITOR_DPI_TYPE};

use crate::core::types::Rect;
use crate::platform::DisplayInfo;

/// Enumerate all monitors.
///
/// Uses `EnumDisplayMonitors` with a callback that collects each monitor's
/// bounds (`GetMonitorInfoW`), DPI (`GetDpiForMonitor` → scale), device name,
/// and primary flag.
///
/// `is_builtin` is always `false` on Windows — there is no cheap Win32 API for
/// detecting built-in panels; EDID via `SetupAPI` would be needed.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if `EnumDisplayMonitors` fails.
pub fn displays() -> Result<Vec<DisplayInfo>, crate::core::errors::ForepawError> {
    let mut collected: Vec<DisplayInfo> = Vec::new();
    let ptr: *mut Vec<DisplayInfo> = &raw mut collected;
    let lparam = LPARAM(ptr as isize);

    // SAFETY: EnumDisplayMonitors enumerates monitors and calls our callback
    // once per monitor with (HMONITOR, HDC, *RECT, LPARAM). We pass `collected`
    // as LPARAM; the callback dereferences it under the same lifetime. hdc and
    // lprcClip are None to enumerate all monitors.
    let result = unsafe { EnumDisplayMonitors(None, None, Some(enum_callback), lparam) };
    result.ok().map_err(|e| {
        crate::core::errors::ForepawError::ActionFailed(format!("EnumDisplayMonitors failed: {e}"))
    })?;
    Ok(collected)
}

/// `EnumDisplayMonitors` callback. Reconstructs the `Vec` pointer from
/// `LPARAM`, queries each monitor, and appends. Returns `BOOL(1)` (continue) in
/// all cases so one failing monitor doesn't abort the rest.
unsafe extern "system" fn enum_callback(
    monitor: windows::Win32::Graphics::Gdi::HMONITOR,
    _hdc: windows::Win32::Graphics::Gdi::HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    // SAFETY: lparam was passed from displays() as a valid pointer to the Vec.
    let out: &mut Vec<DisplayInfo> = unsafe { &mut *(lparam.0 as *mut Vec<DisplayInfo>) };

    if let Some(info) = query_monitor(monitor) {
        out.push(info);
    }
    BOOL(1) // continue enumeration
}

/// Query a single monitor's properties.
///
/// Returns `None` if any Win32 call fails for this monitor (so enumeration
/// can continue with the rest).
fn query_monitor(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Option<DisplayInfo> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = u32::try_from(size_of::<MONITORINFOEXW>()).ok()?;
    // SAFETY: monitor is a valid HMONITOR from the enumeration callback; info
    // is a valid MONITORINFOEXW whose first field is the MONITORINFO the API
    // reads, with cbSize set correctly.
    let ok = unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast::<MONITORINFO>()) }.as_bool();
    if !ok {
        return None;
    }
    let rc = info.monitorInfo.rcMonitor;
    let scale_factor = {
        let mut dpi_x = 0_u32;
        let mut dpi_y = 0_u32;
        // SAFETY: monitor is valid; GetDpiForMonitor writes to caller buffers.
        let r = unsafe {
            GetDpiForMonitor(monitor, MONITOR_DPI_TYPE(0), &raw mut dpi_x, &raw mut dpi_y)
        };
        if r.is_err() {
            1.0
        } else {
            f64::from(dpi_x) / 96.0
        }
    };
    let is_primary = (info.monitorInfo.dwFlags & 0x0000_0001) != 0;
    Some(DisplayInfo {
        id: id_from_hmonitor(monitor),
        name: device_name(&info),
        logical_bounds: Rect::new(
            f64::from(rc.left),
            f64::from(rc.top),
            f64::from(rc.right - rc.left),
            f64::from(rc.bottom - rc.top),
        ),
        scale_factor,
        is_primary,
        is_builtin: None,
        color_space: color_space(&info),
        refresh_rate_hz: refresh_rate_hz(&info),
        // HDR needs `QueryDisplayConfig` + `DisplayConfigGetDeviceInfo`
        // (AdvancedColorSupported) — ~60 lines, untestable without real HDR
        // hardware. Defer until a consumer needs it.
        is_hdr: None,
        is_hdr_active: None,
    })
}

/// ICC profile name (without extension) for the monitor's color space.
///
/// Creates a per-monitor DC via `CreateDCW` (using the GDI device name), then
/// reads the ICC profile path via `GetICMProfileW`. The filename portion
/// (e.g. "sRGB Color Space Profile") is returned — this is Windows's closest
/// equivalent to macOS's `NSColorSpace.localizedName`. Returns `None` if the
/// DC cannot be created or the profile is empty.
fn color_space(info: &MONITORINFOEXW) -> Option<String> {
    // SAFETY: CreateDCW with a valid device name returns an HDC. The device
    // name is nul-terminated in szDevice.
    let hdc = unsafe {
        CreateDCW(
            None,
            windows::core::PCWSTR(info.szDevice.as_ptr()),
            None,
            None,
        )
    };
    if hdc.is_invalid() {
        return None;
    }

    let mut buf = [0u16; 260];
    let mut size = u32::try_from(buf.len()).ok()?;
    // SAFETY: hdc is valid (checked above); buf and size are valid.
    let ok = unsafe {
        GetICMProfileW(
            hdc,
            &raw mut size,
            Some(windows::core::PWSTR(buf.as_mut_ptr())),
        )
    }
    .as_bool();
    // SAFETY: hdc was created by CreateDCW and must be released.
    unsafe { DeleteDC(hdc).ok().unwrap_or_default() };

    if !ok {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0)?;
    let path = String::from_utf16(buf.get(..len)?).ok()?;
    // Extract filename without extension (e.g. "sRGB Color Space Profile").
    let filename = std::path::Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_owned);
    filename.filter(|s| !s.is_empty())
}

/// Extract the device name (`szDevice`, e.g. `\\.\DISPLAY1`) from monitor info.
///
/// This is the adapter device name, not the friendly monitor name ("DELL
/// U2723QE"); the latter requires EDID via `SetupAPI`. Included as-is because it
/// still distinguishes displays and matches the Win32 convention.
fn device_name(info: &MONITORINFOEXW) -> Option<String> {
    let s = info.szDevice;
    // szDevice is a fixed [u16; 32] array, nul-terminated.
    let len = s.iter().position(|&c| c == 0)?;
    String::from_utf16(s.get(..len)?).ok()
}

/// Refresh rate (Hz) from the current display mode.
///
/// Uses `EnumDisplaySettingsW` with the monitor's GDI device name
/// (`szDevice`, e.g. `\\.\DISPLAY1`) and reads `dmDisplayFrequency`. Returns
/// `None` if the call fails or reports 0/1 (Windows uses 0 or 1 for the
/// "default" refresh rate when the actual value is unknown).
fn refresh_rate_hz(info: &MONITORINFOEXW) -> Option<f64> {
    let mut devmode = DEVMODEW {
        dmSize: u16::try_from(size_of::<DEVMODEW>()).ok()?,
        ..Default::default()
    };
    // SAFETY: szDevice is a valid nul-terminated wide string from MONITORINFOEXW;
    // devmode is a valid DEVMODEW with dmSize set.
    let ok = unsafe {
        EnumDisplaySettingsW(
            windows::core::PCWSTR(info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &raw mut devmode,
        )
    };
    if !ok.as_bool() {
        return None;
    }
    let hz = devmode.dmDisplayFrequency;
    // 0 or 1 means "default/unknown" — not a real refresh rate.
    if hz <= 1 {
        return None;
    }
    Some(f64::from(hz))
}

/// Pack an `HMONITOR` handle into the `u32` id field.
fn id_from_hmonitor(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "HMONITOR values are small handle indices that fit in u32"
    )]
    let id = monitor.0 as usize as u32;
    id
}
