//! UIA hit test: element at point via `IUIAutomation::ElementFromPoint`.
//!
//! Retrieves the deepest accessible element at a screen coordinate and walks
//! its parent chain using `ControlViewWalker::GetParentElement`.
//!
//! Coordinate system: screen coordinates (desktop-wide), matching the
//! `POINT` parameter of `ElementFromPoint`. The caller converts from
//! per-window coords before calling the trait method.

use windows::Win32::Foundation::POINT;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
};

use crate::core::errors::ForepawError;
use crate::core::types::Point;
use crate::platform::{AncestorInfo, HitTestResult};

use super::snapshot::{control_type_to_role, get_element_bounds};

/// Performs a hit test at the given screen coordinates.
///
/// Returns the deepest UIA element at the point and its ancestor chain.
/// Uses `IUIAutomation::ElementFromPoint` for the hit test and
/// `ControlViewWalker::GetParentElement` for the ancestor chain.
///
/// # Errors
///
/// Returns [`ForepawError::ActionFailed`] if no element is found at the position
/// or UIA fails (COM init missing, element disappeared, etc).
pub fn element_at_point(
    point: Point,
    _app_hint: Option<&str>,
) -> Result<HitTestResult, ForepawError> {
    // 1. Create UIA instance
    // SAFETY: CoCreateInstance with CUIAutomation CLSID is a standard COM operation.
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).map_err(|e| {
            ForepawError::ActionFailed(format!("failed to create IUIAutomation: {e}"))
        })?
    };

    // 2. Convert to POINT (screen coordinates, i32 from f64)
    #[expect(
        clippy::cast_possible_truncation,
        reason = "screen coordinates fit in i32 without meaningful loss"
    )]
    let pt = POINT {
        x: point.x as i32,
        y: point.y as i32,
    };

    // 3. Perform the hit test
    // SAFETY: ElementFromPoint is a standard UIA call on a valid instance.
    let hit_element: IUIAutomationElement = unsafe {
        automation.ElementFromPoint(pt).map_err(|e| {
            let msg = format!("ElementFromPoint at ({}, {}): {e}", pt.x, pt.y);
            ForepawError::ActionFailed(msg)
        })?
    };

    // 4. Get basic properties
    let role = control_type_to_role(get_control_type_id(&hit_element)).to_owned();
    // SAFETY: CurrentName on a valid UIA element from ElementFromPoint.
    let name = get_bstr_property(&hit_element, |e| unsafe { e.CurrentName() });
    let bounds = get_element_bounds(&hit_element);
    let pid = get_process_id(&hit_element);

    // 5. Get value via Value pattern
    let value = get_value_pattern(&hit_element);

    // 6. Create tree walker for parent chain
    // SAFETY: ControlViewWalker is a standard UIA operation.
    let walker: IUIAutomationTreeWalker = unsafe {
        automation.ControlViewWalker().map_err(|e| {
            ForepawError::ActionFailed(format!("ControlViewWalker failed: {e}"))
        })?
    };

    // 7. Walk parent chain via ControlViewWalker
    let mut ancestors: Vec<AncestorInfo> = Vec::new();
    // SAFETY: GetParentElement on a valid UIA element.
    let mut current = unsafe { walker.GetParentElement(&hit_element).ok() };

    while let Some(ref element) = current {
        let parent_role = control_type_to_role(get_control_type_id(element)).to_owned();
        // SAFETY: CurrentName on a valid UIA element from GetParentElement chain.
        let parent_name =
            get_bstr_property(element, |e| unsafe { e.CurrentName() }).filter(|s| !s.is_empty());
        let parent_bounds = get_element_bounds(element);

        ancestors.push(AncestorInfo {
            role: parent_role.clone(),
            name: parent_name,
            bounds: parent_bounds,
        });

        // Stop at the desktop/root: process id 0 indicates root element
        // SAFETY: GetParentElement on a valid UIA element.
        let next = unsafe { walker.GetParentElement(element).ok() };
        let should_stop = next.as_ref().is_none_or(|n| get_process_id(n) == 0);

        if should_stop {
            break;
        }
        current = next;
    }
    ancestors.reverse(); // root-first

    // 8. Detect available patterns as action names
    let actions = get_available_patterns(&hit_element);

    Ok(HitTestResult {
        role,
        name,
        value,
        bounds,
        actions,
        ancestors,
        pid,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the raw `ControlType` ID from a UIA element.
fn get_control_type_id(element: &IUIAutomationElement) -> i32 {
    // SAFETY: CurrentControlType on a valid UIA element.
    unsafe { element.CurrentControlType().map_or(0, |ct| ct.0) }
}

/// Get the process ID from a UIA element.
fn get_process_id(element: &IUIAutomationElement) -> i32 {
    // SAFETY: CurrentProcessId on a valid UIA element.
    unsafe { element.CurrentProcessId().unwrap_or(0) }
}

/// Get a BSTR-returning property from a UIA element.
fn get_bstr_property(
    element: &IUIAutomationElement,
    f: impl FnOnce(&IUIAutomationElement) -> windows::core::Result<windows::core::BSTR>,
) -> Option<String> {
    let bstr = f(element).ok()?;
    let s = bstr.to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Get the element value via the Value pattern.
fn get_value_pattern(element: &IUIAutomationElement) -> Option<String> {
    use windows::Win32::UI::Accessibility::{IUIAutomationValuePattern, UIA_ValuePatternId};

    // SAFETY: GetCurrentPatternAs with the correct pattern type.
    let pattern = unsafe {
        element
            .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            .ok()
    }?;

    // SAFETY: CurrentValue on a valid Value pattern.
    let value = unsafe { pattern.CurrentValue() }.ok()?;
    let s = value.to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Detect available control patterns and return their names as action strings.
fn get_available_patterns(element: &IUIAutomationElement) -> Vec<String> {
    use windows::Win32::UI::Accessibility::{
        IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
        IUIAutomationRangeValuePattern, IUIAutomationScrollPattern,
        IUIAutomationSelectionItemPattern, IUIAutomationTextPattern,
        IUIAutomationTogglePattern, IUIAutomationValuePattern, IUIAutomationWindowPattern,
        UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_RangeValuePatternId,
        UIA_ScrollPatternId, UIA_SelectionItemPatternId, UIA_TextPatternId,
        UIA_TogglePatternId, UIA_ValuePatternId, UIA_WindowPatternId,
    };

    let mut actions = Vec::new();

    /// Macro to check if a specific pattern is available.
    macro_rules! check_pattern {
        ($type:ty, $id:expr) => {{
            // SAFETY: ElementFromPoint returns a valid element.
            // GetCurrentPatternAs fails gracefully if pattern unsupported.
            unsafe { element.GetCurrentPatternAs::<$type>($id).is_ok() }
        }};
    }

    if check_pattern!(IUIAutomationInvokePattern, UIA_InvokePatternId) {
        actions.push("Invoke".to_owned());
    }
    if check_pattern!(IUIAutomationValuePattern, UIA_ValuePatternId) {
        actions.push("SetValue".to_owned());
    }
    if check_pattern!(IUIAutomationSelectionItemPattern, UIA_SelectionItemPatternId) {
        actions.push("Select".to_owned());
    }
    if check_pattern!(IUIAutomationScrollPattern, UIA_ScrollPatternId) {
        actions.push("Scroll".to_owned());
    }
    if check_pattern!(IUIAutomationExpandCollapsePattern, UIA_ExpandCollapsePatternId) {
        actions.push("ExpandCollapse".to_owned());
    }
    if check_pattern!(IUIAutomationTogglePattern, UIA_TogglePatternId) {
        actions.push("Toggle".to_owned());
    }
    if check_pattern!(IUIAutomationWindowPattern, UIA_WindowPatternId) {
        actions.push("Window".to_owned());
    }
    if check_pattern!(IUIAutomationRangeValuePattern, UIA_RangeValuePatternId) {
        actions.push("RangeValue".to_owned());
    }
    if check_pattern!(IUIAutomationTextPattern, UIA_TextPatternId) {
        actions.push("Text".to_owned());
    }

    actions
}
