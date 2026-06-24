//! Physical display / monitor types and display-region lookup.
//!
//! [`DisplayInfo`] is the platform-agnostic description of a monitor — plain
//! data over core types ([`Rect`], scalars, `Option<String>`). Platform
//! backends *populate* it; the type itself is core. [`display_for_bounds`]
//! derives which display a rectangular region (a window) sits on, used by the
//! screenshot path to report the correct per-display backing scale rather than
//! assuming the main display's.

use crate::core::types::Rect;

/// Info about a physical display / monitor.
#[derive(Debug, Clone, serde::Serialize)]
#[non_exhaustive]
pub struct DisplayInfo {
    /// Platform display identifier (e.g. macOS `CGDirectDisplayID`).
    pub id: u32,
    /// Human-readable name ("Color LCD", "DELL U2723QE"). `None` on platforms
    /// without a cheap name lookup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Bounds in the global logical/point coordinate space (the same space as
    /// window and element bounds).
    pub logical_bounds: Rect,
    /// Backing scale factor (1.0 or 2.0 on macOS; fractional possible on
    /// Windows/Linux). Multiply logical sizes by this to get pixel sizes.
    pub scale_factor: f64,
    /// Whether this is a primary/main display.
    pub is_primary: bool,
    /// Whether this is a built-in display (laptop lid, iPad Sidecar).
    /// `None` where the platform does not expose built-in detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_builtin: Option<bool>,
    /// Color space name ("Display P3", "sRGB"). `None` where the platform has
    /// no cheap name lookup (Windows exposes only an ICC profile path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
    /// Maximum refresh rate in Hz. Integer-rounded on macOS (`NSInteger`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_rate_hz: Option<f64>,
    /// Whether the display supports HDR / EDR content (extended dynamic range).
    /// Hardware capability — the panel can reproduce values beyond standard
    /// sRGB white.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_hdr: Option<bool>,
    /// Whether HDR / EDR is currently active on the display. Runtime state —
    /// an HDR-capable panel reports `false` when EDR isn't engaged (the common
    /// case, since macOS leaves EDR off until an app opts in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_hdr_active: Option<bool>,
}

/// Find the display that best contains a rectangular region.
///
/// Uses **majority overlap area**: returns the display whose intersection
/// with `bounds` has the largest area. This is the only sane rule for windows
/// spanning two displays — a window straddling a Retina and a 1x display has no
/// single "correct" scale, so the majority wins. Ties (equal overlap area) go
/// to the last display in iteration order (per `Iterator::max_by`); either is
/// arbitrary for a true 50/50 split. Returns `None` if `bounds`
/// overlaps no display.
///
/// Used to derive the per-display backing scale for a captured window rather
/// than assuming the main display's scale (which is wrong for windows on a
/// non-primary display).
#[must_use]
pub fn display_for_bounds(displays: &[DisplayInfo], bounds: Rect) -> Option<&DisplayInfo> {
    displays
        .iter()
        .filter_map(|d| Some((d.logical_bounds.intersect(bounds)?.area(), d)))
        .max_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, d)| d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: u32, bounds: Rect, scale: f64) -> DisplayInfo {
        DisplayInfo {
            id,
            name: None,
            logical_bounds: bounds,
            scale_factor: scale,
            is_primary: false,
            is_builtin: None,
            color_space: None,
            refresh_rate_hz: None,
            is_hdr: None,
            is_hdr_active: None,
        }
    }

    #[test]
    fn display_for_bounds_picks_containing_display() {
        let displays = [
            display(1, Rect::new(0.0, 0.0, 1440.0, 900.0), 2.0),
            display(2, Rect::new(1440.0, 0.0, 1024.0, 768.0), 2.0),
        ];
        // Window entirely on the second display.
        let win = Rect::new(1600.0, 100.0, 400.0, 400.0);
        assert_eq!(display_for_bounds(&displays, win).unwrap().id, 2);
    }

    #[test]
    fn display_for_bounds_majority_overlap_wins() {
        let displays = [
            display(1, Rect::new(0.0, 0.0, 1000.0, 1000.0), 2.0),
            display(2, Rect::new(1000.0, 0.0, 1000.0, 1000.0), 1.0),
        ];
        // Equal overlap -> last display wins per max_by (arbitrary for a true tie).
        let tied = Rect::new(800.0, 0.0, 400.0, 1000.0); // 200pt on each
        assert_eq!(display_for_bounds(&displays, tied).unwrap().id, 2);

        // Majority on display 2.
        let win2 = Rect::new(800.0, 0.0, 800.0, 1000.0); // 200pt on d1, 600pt on d2
        assert_eq!(display_for_bounds(&displays, win2).unwrap().id, 2);
    }

    #[test]
    fn display_for_bounds_no_overlap_is_none() {
        let displays = [display(1, Rect::new(0.0, 0.0, 1000.0, 1000.0), 2.0)];
        let win = Rect::new(2000.0, 2000.0, 100.0, 100.0);
        assert!(display_for_bounds(&displays, win).is_none());
    }

    #[test]
    fn display_for_bounds_empty_displays_is_none() {
        assert!(display_for_bounds(&[], Rect::new(0.0, 0.0, 100.0, 100.0)).is_none());
    }
}
