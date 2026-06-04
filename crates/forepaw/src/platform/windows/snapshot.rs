//! UI Automation tree snapshot: walk the UIA tree and build an `ElementTree`.
//!
//! Uses `IUIAutomation` + `ControlView` `TreeWalker` for tree navigation.
//! Maps UIA `ControlType` IDs to `Role` variants so the core
//! ref assigner and tree renderer work unchanged across platforms.
//!
//! Control type IDs from:
//! <https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controltype-ids>

use windows::Win32::Foundation::RECT;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
};

use crate::core::element_tree::{ElementData, ElementNode, ElementTree, SnapshotTiming};
use crate::core::errors::ForepawError;
use crate::core::ref_assigner::RefAssigner;
use crate::core::role::Role;
use crate::core::types::Rect;
use crate::platform::{AppTarget, SnapshotOptions, WindowTarget};

use super::app;
use super::role::control_type_to_role;

/// Initialize COM for UIA calls.
///
/// Called once during `WindowsProvider` construction. Ok to call multiple times
/// (subsequent calls return `S_FALSE` but are harmless).
pub fn init_com() {
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().ok();
    }
}

// ---------------------------------------------------------------------------
// Tree pruning
// ---------------------------------------------------------------------------

struct TreePruning {
    skip_zero_size: bool,
    skip_offscreen: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Walk the UIA tree for the given app and return an `ElementTree`.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running.
pub fn snapshot(
    app: &AppTarget,
    window: Option<&WindowTarget>,
    options: &SnapshotOptions,
) -> Result<ElementTree, ForepawError> {
    // Find the target window
    let (hwnd, _) = app::find_app_hwnd(app, window)?;

    // Bring to foreground (may be needed for accurate tree content)
    app::activate_app(hwnd);

    // Create UIA instance
    // SAFETY: UIA tree traversal on valid element.
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(
            &CUIAutomation,
            None,
            windows::Win32::System::Com::CLSCTX_ALL,
        )
        .map_err(|e| ForepawError::ActionFailed(format!("failed to create IUIAutomation: {e}")))?
    };

    // Get element from window handle
    // SAFETY: UIA tree traversal on valid element.
    let root_element: IUIAutomationElement = unsafe {
        automation
            .ElementFromHandle(hwnd)
            .map_err(|e| ForepawError::ActionFailed(format!("ElementFromHandle failed: {e}")))?
    };

    // Use ControlView TreeWalker (closest to macOS pruned tree)
    // SAFETY: UIA tree traversal on valid element.
    let walker: IUIAutomationTreeWalker = unsafe {
        automation
            .ControlViewWalker()
            .map_err(|e| ForepawError::ActionFailed(format!("ControlViewWalker failed: {e}")))?
    };

    // Get window bounds from UIA for coordinate display
    let window_bounds = get_element_bounds(&root_element);

    let pruning = TreePruning {
        skip_zero_size: options.skip_zero_size,
        skip_offscreen: options.skip_offscreen,
    };

    let effective_depth = options.max_depth;

    let walk_start = std::time::Instant::now();
    let root = build_tree(&walker, &root_element, 0, effective_depth, &pruning);
    let walk_ms = walk_start.elapsed().as_secs_f64() * 1000.0;

    let assigner = RefAssigner::new();
    let result = assigner.assign(&root, options.interactive_only);

    let timing = if options.timing {
        let node_count = SnapshotTiming::count_nodes(&result.root);
        Some(SnapshotTiming::new(
            walk_ms,
            node_count,
            result.root.clone(),
        ))
    } else {
        None
    };

    Ok(ElementTree {
        app: app.display(),
        root: result.root,
        refs: result.refs,
        window_bounds,
        timing,
    })
}

// ---------------------------------------------------------------------------
// Tree walk
// ---------------------------------------------------------------------------

fn build_tree(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    depth: usize,
    max_depth: usize,
    pruning: &TreePruning,
) -> ElementNode {
    if depth >= max_depth {
        return ElementNode::new(ElementData::new(Role::Group));
    }

    // Read properties via direct accessors (no VARIANT/SAFEARRAY)
    let role = get_control_type(element);
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let name = get_bstr_property(element, |e| unsafe { e.CurrentName() });
    let bounds = get_element_bounds(element);

    // Prune zero-size elements (collapsed menus, hidden panels)
    if pruning.skip_zero_size {
        if let Some(b) = &bounds {
            if b.width == 0.0 && b.height == 0.0 && depth > 1 {
                return ElementNode::new(
                    ElementData::new(role)
                        .with_name_opt(non_empty(name.as_ref()))
                        .with_bounds(*b),
                );
            }
        }
    }

    // Prune offscreen elements
    if pruning.skip_offscreen && depth > 1 && is_offscreen(element) {
        return ElementNode::new(
            ElementData::new(role)
                .with_name_opt(non_empty(name.as_ref()))
                .with_bounds_opt(bounds),
        );
    }

    // Walk children via TreeWalker (depth-first)
    let children = walk_children(walker, element, depth, max_depth, pruning);

    // Name resolution: CurrentName → CurrentHelpText → first text child
    let computed_name = if name.as_ref().is_none_or(String::is_empty) {
        resolve_name(element, &children)
    } else {
        None
    };

    let final_name = non_empty(name.as_ref()).or(computed_name);

    ElementNode {
        data: ElementData {
            role,
            name: final_name,
            value: None, // TODO: UIA Value pattern for element values
            reference: None,
            bounds,
            // TODO: populate from UIA (IsEnabled, HasKeyboardFocus, IsSelected, HelpText)
            enabled: None,
            focused: None,
            selected: None,
            description: None,
            native_role: None,
            identifier: None,
            uid: None,
            signature: None,
            signature_bounds: None,
            attributes: Vec::new(),
        },
        children,
    }
}

/// Walk children using `TreeWalker` GetFirstChild/GetNextSibling.
fn walk_children(
    walker: &IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    parent_depth: usize,
    max_depth: usize,
    pruning: &TreePruning,
) -> Vec<ElementNode> {
    let mut children = Vec::new();

    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let Ok(first_child) = (unsafe { walker.GetFirstChildElement(parent) }) else {
        return children;
    };

    children.push(build_tree(
        walker,
        &first_child,
        parent_depth + 1,
        max_depth,
        pruning,
    ));

    let mut current = first_child;
    loop {
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        let Ok(next) = (unsafe { walker.GetNextSiblingElement(&current) }) else {
            break;
        };
        children.push(build_tree(
            walker,
            &next,
            parent_depth + 1,
            max_depth,
            pruning,
        ));
        current = next;
    }

    children
}

// ---------------------------------------------------------------------------
// Name resolution
// ---------------------------------------------------------------------------

/// Resolve element name when `CurrentName` is empty.
/// Chain: `HelpText` → first child with text content.
fn resolve_name(element: &IUIAutomationElement, children: &[ElementNode]) -> Option<String> {
    // 1. HelpText
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let help = get_bstr_property(element, |e| unsafe { e.CurrentHelpText() });
    if let Some(h) = non_empty(help.as_ref()) {
        return Some(h);
    }

    // 2. First child that looks like a text label (`StaticText` with a name)
    for child in children {
        if child.data.role == Role::StaticText {
            if let Some(ref name) = child.data.name {
                if !name.is_empty() {
                    return Some(name.clone());
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Property accessors
// ---------------------------------------------------------------------------

/// Get the element's `ControlType`, mapped to a `Role` variant.
fn get_control_type(element: &IUIAutomationElement) -> Role {
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let ct = unsafe { element.CurrentControlType().map_or(0, |ct| ct.0) };
    control_type_to_role(ct)
}

/// Get a BSTR-returning property from a UIA element.
fn get_bstr_property(
    element: &IUIAutomationElement,
    f: impl FnOnce(&IUIAutomationElement) -> windows::core::Result<windows::core::BSTR>,
) -> Option<String> {
    let bstr = f(element).ok()?;
    let s = bstr.to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Get the bounding rectangle from a UIA element.
/// Uses `CurrentBoundingRectangle` which returns a RECT directly.
pub(super) fn get_element_bounds(element: &IUIAutomationElement) -> Option<Rect> {
    // SAFETY: UIA bounding rect read on valid element.
    let rect: RECT = unsafe { element.CurrentBoundingRectangle().ok() }?;

    let r = Rect::new(
        f64::from(rect.left),
        f64::from(rect.top),
        f64::from(rect.right - rect.left),
        f64::from(rect.bottom - rect.top),
    );

    if r.width <= 0.0 || r.height <= 0.0 {
        return None;
    }

    Some(r)
}

/// Check if an element is offscreen.
fn is_offscreen(element: &IUIAutomationElement) -> bool {
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        element
            .CurrentIsOffscreen()
            .is_ok_and(windows::core::BOOL::as_bool)
    }
}

/// Return None for empty strings.
fn non_empty(s: Option<&String>) -> Option<String> {
    s.and_then(|v| if v.is_empty() { None } else { Some(v.clone()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_type_mapping_covers_common_types() {
        assert_eq!(control_type_to_role(50000), Role::Button);
        assert_eq!(control_type_to_role(50004), Role::TextField);
        assert_eq!(control_type_to_role(50002), Role::CheckBox);
        assert_eq!(control_type_to_role(50003), Role::ComboBox);
        assert_eq!(control_type_to_role(50011), Role::MenuItem);
        assert_eq!(control_type_to_role(50013), Role::RadioButton);
        assert_eq!(control_type_to_role(50015), Role::Slider);
        assert_eq!(control_type_to_role(50032), Role::Window);
        assert_eq!(control_type_to_role(50026), Role::Group);
        assert_eq!(control_type_to_role(50020), Role::StaticText);
        assert_eq!(control_type_to_role(50008), Role::List);
        assert_eq!(control_type_to_role(50036), Role::Table);
    }

    #[test]
    fn unknown_control_type_maps_to_unknown() {
        assert_eq!(control_type_to_role(0), Role::Unknown);
        assert_eq!(control_type_to_role(99999), Role::Unknown);
    }

    #[test]
    fn non_empty_returns_none_for_empty() {
        assert_eq!(non_empty(None::<&String>), None);
        assert_eq!(non_empty(Some(&String::new())), None);
        assert_eq!(
            non_empty(Some(&"hello".to_owned())),
            Some("hello".to_owned())
        );
    }

    #[test]
    fn interactive_roles_mapped_correctly() {
        assert!(control_type_to_role(50000).is_interactive()); // Button
        assert!(control_type_to_role(50004).is_interactive()); // Edit/TextField
        assert!(control_type_to_role(50002).is_interactive()); // CheckBox
        assert!(control_type_to_role(50003).is_interactive()); // ComboBox
        assert!(control_type_to_role(50011).is_interactive()); // MenuItem
        assert!(control_type_to_role(50013).is_interactive()); // RadioButton
        assert!(control_type_to_role(50015).is_interactive()); // Slider
        assert!(control_type_to_role(50024).is_interactive()); // TreeItem
    }

    #[test]
    fn structural_roles_not_interactive() {
        assert!(!control_type_to_role(50032).is_interactive()); // Window
        assert!(!control_type_to_role(50026).is_interactive()); // Group
        assert!(!control_type_to_role(50020).is_interactive()); // Text
        assert!(!control_type_to_role(50006).is_interactive()); // Image
    }
}
