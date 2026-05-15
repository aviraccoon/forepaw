//! UI Automation tree snapshot: walk the UIA tree and build an ElementTree.
//!
//! Uses IUIAutomation + ControlView TreeWalker for tree navigation.
//! Maps UIA ControlType IDs to AX-prefixed role names so the core
//! ref assigner and tree renderer work unchanged across platforms.
//!
//! Control type IDs from:
//! https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controltype-ids

use windows::Win32::Foundation::RECT;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
};

use crate::core::element_tree::{ElementNode, ElementTree, SnapshotTiming};
use crate::core::errors::ForepawError;
use crate::core::ref_assigner::RefAssigner;
use crate::core::types::Rect;
use crate::platform::SnapshotOptions;

use super::app;

/// UIA ControlType IDs mapped to AX-style role names.
///
/// Names follow macOS convention so `is_interactive_role()` and the
/// tree renderer work cross-platform. UIA types without a close AX
/// equivalent map to the nearest structural equivalent or AXUnknown.
///
/// Source: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controltype-ids
fn control_type_to_role(control_type: i32) -> &'static str {
    match control_type {
        50000 => "AXButton",                             // Button
        50001 => "AXCalendar",                           // Calendar
        50002 => "AXCheckBox",                           // CheckBox
        50003 => "AXComboBox",                           // ComboBox
        50004 => "AXTextField",                          // Edit
        50005 => "AXLink",                               // Hyperlink
        50006 => "AXImage",                              // Image
        50007 | 50029 => "AXCell",                       // ListItem, DataItem
        50008 => "AXList",                               // List
        50009 => "AXMenu",                               // Menu
        50010 => "AXMenuBar",                            // MenuBar
        50011 => "AXMenuItem",                           // MenuItem
        50012 => "AXProgressIndicator",                  // ProgressBar
        50013 => "AXRadioButton",                        // RadioButton
        50014 => "AXScrollBar",                          // ScrollBar
        50015 => "AXSlider",                             // Slider
        50016 => "AXIncrementor",                        // Spinner
        50017 | 50020 | 50035 | 50037 => "AXStaticText", // StatusBar, Text, HeaderItem, TitleBar
        50018 => "AXTabGroup",                           // Tab (container)
        50019 => "AXTab",                                // TabItem
        50021 => "AXToolbar",                            // ToolBar
        50023 => "AXOutline",                            // Tree
        50024 => "AXTreeItem",                           // TreeItem
        50026 | 50033 | 50034 | 50041 => "AXGroup",      // Group, Pane, Header, Pane (dup)
        50028 | 50036 => "AXTable",                      // DataGrid, Table
        50030 => "AXTextArea",                           // Document
        50031 => "AXMenuButton",                         // SplitButton
        50032 => "AXWindow",                             // Window
        // ToolTip(50022), Custom(50025), Thumb(50027), Separator(50038),
        // SemanticZoom(50039), AppBar(50040), and anything else
        _ => "AXUnknown",
    }
}

/// Initialize COM for UIA calls.
///
/// Called once during WindowsProvider construction. Ok to call multiple times
/// (subsequent calls return S_FALSE but are harmless).
pub fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
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

/// Walk the UIA tree for the given app and return an ElementTree.
pub fn snapshot(app_name: &str, options: &SnapshotOptions) -> Result<ElementTree, ForepawError> {
    // Find the target window
    let (hwnd, _) = app::find_app_hwnd(app_name)?;

    // Bring to foreground (may be needed for accurate tree content)
    app::activate_app(hwnd);

    // Create UIA instance
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(
            &CUIAutomation,
            None,
            windows::Win32::System::Com::CLSCTX_ALL,
        )
        .map_err(|e| ForepawError::ActionFailed(format!("failed to create IUIAutomation: {e}")))?
    };

    // Get element from window handle
    let root_element: IUIAutomationElement = unsafe {
        automation
            .ElementFromHandle(hwnd)
            .map_err(|e| ForepawError::ActionFailed(format!("ElementFromHandle failed: {e}")))?
    };

    // Use ControlView TreeWalker (closest to macOS pruned tree)
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
        app: app_name.to_string(),
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
        return ElementNode::new("AXGroup");
    }

    // Read properties via direct accessors (no VARIANT/SAFEARRAY)
    let role = get_control_type(element);
    let name = get_bstr_property(element, |e| unsafe { e.CurrentName() });
    let bounds = get_element_bounds(element);

    // Prune zero-size elements (collapsed menus, hidden panels)
    if pruning.skip_zero_size {
        if let Some(b) = &bounds {
            if b.width == 0.0 && b.height == 0.0 && depth > 1 {
                return ElementNode {
                    role,
                    name: non_empty(&name),
                    value: None,
                    r#ref: None,
                    bounds,
                    attributes: Vec::new(),
                    children: Vec::new(),
                };
            }
        }
    }

    // Prune offscreen elements
    if pruning.skip_offscreen && depth > 1 && is_offscreen(element) {
        return ElementNode {
            role,
            name: non_empty(&name),
            value: None,
            r#ref: None,
            bounds,
            attributes: Vec::new(),
            children: Vec::new(),
        };
    }

    // Walk children via TreeWalker (depth-first)
    let children = walk_children(walker, element, depth, max_depth, pruning);

    // Name resolution: CurrentName → CurrentHelpText → first text child
    let computed_name = if name.as_ref().is_none_or(String::is_empty) {
        resolve_name(element, &children)
    } else {
        None
    };

    let final_name = non_empty(&name).or(computed_name);

    ElementNode {
        role,
        name: final_name,
        value: None, // TODO: UIA Value pattern for element values
        r#ref: None,
        bounds,
        attributes: Vec::new(),
        children,
    }
}

/// Walk children using TreeWalker GetFirstChild/GetNextSibling.
fn walk_children(
    walker: &IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    parent_depth: usize,
    max_depth: usize,
    pruning: &TreePruning,
) -> Vec<ElementNode> {
    let mut children = Vec::new();

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

/// Resolve element name when CurrentName is empty.
/// Chain: HelpText → first child with text content.
fn resolve_name(element: &IUIAutomationElement, children: &[ElementNode]) -> Option<String> {
    // 1. HelpText
    let help = get_bstr_property(element, |e| unsafe { e.CurrentHelpText() });
    if let Some(h) = non_empty(&help) {
        return Some(h);
    }

    // 2. First child that looks like a text label (AXStaticText with a name)
    for child in children {
        if child.role == "AXStaticText" {
            if let Some(ref name) = child.name {
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

/// Get the element's ControlType, mapped to an AX-style role name.
fn get_control_type(element: &IUIAutomationElement) -> String {
    let ct = unsafe { element.CurrentControlType().map(|ct| ct.0).unwrap_or(0) };
    control_type_to_role(ct).to_string()
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
/// Uses CurrentBoundingRectangle which returns a RECT directly.
fn get_element_bounds(element: &IUIAutomationElement) -> Option<Rect> {
    let rect: RECT = unsafe { element.CurrentBoundingRectangle().ok() }?;

    let r = Rect::new(
        rect.left as f64,
        rect.top as f64,
        (rect.right - rect.left) as f64,
        (rect.bottom - rect.top) as f64,
    );

    if r.width <= 0.0 || r.height <= 0.0 {
        return None;
    }

    Some(r)
}

/// Check if an element is offscreen.
fn is_offscreen(element: &IUIAutomationElement) -> bool {
    unsafe {
        element
            .CurrentIsOffscreen()
            .map(windows::core::BOOL::as_bool)
            .unwrap_or(false)
    }
}

/// Return None for empty strings.
fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .and_then(|v| if v.is_empty() { None } else { Some(v.clone()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_type_mapping_covers_common_types() {
        assert_eq!(control_type_to_role(50000), "AXButton");
        assert_eq!(control_type_to_role(50004), "AXTextField");
        assert_eq!(control_type_to_role(50002), "AXCheckBox");
        assert_eq!(control_type_to_role(50003), "AXComboBox");
        assert_eq!(control_type_to_role(50011), "AXMenuItem");
        assert_eq!(control_type_to_role(50013), "AXRadioButton");
        assert_eq!(control_type_to_role(50015), "AXSlider");
        assert_eq!(control_type_to_role(50032), "AXWindow");
        assert_eq!(control_type_to_role(50026), "AXGroup");
        assert_eq!(control_type_to_role(50020), "AXStaticText");
        assert_eq!(control_type_to_role(50008), "AXList");
        assert_eq!(control_type_to_role(50036), "AXTable");
    }

    #[test]
    fn unknown_control_type_maps_to_ax_unknown() {
        assert_eq!(control_type_to_role(0), "AXUnknown");
        assert_eq!(control_type_to_role(99999), "AXUnknown");
    }

    #[test]
    fn non_empty_returns_none_for_empty() {
        assert_eq!(non_empty(&None), None);
        assert_eq!(non_empty(&Some("".to_string())), None);
        assert_eq!(
            non_empty(&Some("hello".to_string())),
            Some("hello".to_string())
        );
    }

    #[test]
    fn interactive_roles_mapped_correctly() {
        // All UIA types that map to interactive AX roles should be detected
        use crate::core::element_tree::is_interactive_role;

        assert!(is_interactive_role(control_type_to_role(50000))); // Button
        assert!(is_interactive_role(control_type_to_role(50004))); // Edit/TextField
        assert!(is_interactive_role(control_type_to_role(50002))); // CheckBox
        assert!(is_interactive_role(control_type_to_role(50003))); // ComboBox
        assert!(is_interactive_role(control_type_to_role(50011))); // MenuItem
        assert!(is_interactive_role(control_type_to_role(50013))); // RadioButton
        assert!(is_interactive_role(control_type_to_role(50015))); // Slider
        assert!(is_interactive_role(control_type_to_role(50024))); // TreeItem
    }

    #[test]
    fn structural_roles_not_interactive() {
        use crate::core::element_tree::is_interactive_role;

        assert!(!is_interactive_role(control_type_to_role(50032))); // Window
        assert!(!is_interactive_role(control_type_to_role(50026))); // Group
        assert!(!is_interactive_role(control_type_to_role(50020))); // Text
        assert!(!is_interactive_role(control_type_to_role(50006))); // Image
    }
}
