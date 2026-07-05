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

use std::collections::HashMap;

use crate::core::element_tree::{
    ElementData, ElementNode, ElementRef, ElementTree, NameSource, SnapshotTiming,
};
use crate::core::errors::ForepawError;
use crate::core::ref_assigner::RefAssigner;
use crate::core::ref_cache::build_ref_handle_map;
use crate::core::role::Role;
use crate::core::types::{Point, Rect};
use crate::platform::{AppTarget, SnapshotOptions, WindowTarget};

use super::app;
use super::role::control_type_to_role;

/// Windows' parallel handle tree: the generic
/// [`core::ref_cache::HandleNode`] carrying an owned `IUIAutomationElement`
/// per node. Built in lockstep with `ElementNode` (same recursion, same pruning
/// early-returns) so its shape is identical.
type HandleNode = crate::core::ref_cache::HandleNode<IUIAutomationElement>;

/// Map from ref id to the retained `IUIAutomationElement`, captured during the
/// snapshot walk. Stored on `WindowsProvider` for O(1) ref resolution. No
/// custom `Drop` -- `IUIAutomationElement` is RAII (`Clone` = `AddRef`,
/// `Drop` = `Release`), so the map releasing its entries balances the clones
/// made during the walk.
#[derive(Debug, Default)]
pub(super) struct RefHandleMap(HashMap<i32, IUIAutomationElement>);

impl RefHandleMap {
    /// Create an empty map.
    pub(super) fn empty() -> Self {
        Self(HashMap::new())
    }

    /// Look up the retained handle for a ref.
    pub(super) fn get(&self, ref_id: i32) -> Option<&IUIAutomationElement> {
        self.0.get(&ref_id)
    }
}

// SAFETY: `IUIAutomationElement` is a COM interface pointer. UIA objects
// obtained on an MTA thread are usable from any MTA thread, and
// `WindowsProvider::new` initializes COM with `COINIT_MULTITHREADED`. The map
// is only touched under its `Mutex`, so sending it to another thread (where COM
// is likewise MTA-initialized) is sound. Mirrors the macOS backend's
// `unsafe impl Send` for `AXUIElementRef`.
unsafe impl Send for RefHandleMap {}

/// Initialize COM for UIA calls.
///
/// Called once during `WindowsProvider` construction. Ok to call multiple times
/// (subsequent calls return `S_FALSE` but are harmless).
pub fn init_com() {
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    unsafe {
        drop(CoInitializeEx(None, COINIT_MULTITHREADED).ok());
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
// UIA setup
// ---------------------------------------------------------------------------

/// Create the UIA client, the root element for the app's window, and a
/// `ControlView` tree walker. Shared by [`snapshot`] and the resolve re-walk.
/// The returned `IUIAutomation` must stay alive in the caller's scope (it is
/// the UIA factory); the walker and root element are independent COM objects.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the app/window cannot be found,
/// or [`ForepawError::ActionFailed`] if UIA setup fails.
fn uia_root(
    app: &AppTarget,
    window: Option<&WindowTarget>,
) -> Result<(IUIAutomation, IUIAutomationTreeWalker, IUIAutomationElement), ForepawError> {
    let (hwnd, _) = app::find_app_hwnd(app, window)?;
    app::activate_window(hwnd);

    // SAFETY: UIA factory creation on a CoInitialize'd thread.
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(
            &CUIAutomation,
            None,
            windows::Win32::System::Com::CLSCTX_ALL,
        )
        .map_err(|e| ForepawError::ActionFailed(format!("failed to create IUIAutomation: {e}")))?
    };

    // SAFETY: ElementFromHandle on a valid HWND.
    let root_element: IUIAutomationElement = unsafe {
        automation
            .ElementFromHandle(hwnd)
            .map_err(|e| ForepawError::ActionFailed(format!("ElementFromHandle failed: {e}")))?
    };

    // Use ControlView TreeWalker (closest to the macOS pruned tree).
    // SAFETY: walker retrieval on a valid UIA instance.
    let walker: IUIAutomationTreeWalker = unsafe {
        automation
            .ControlViewWalker()
            .map_err(|e| ForepawError::ActionFailed(format!("ControlViewWalker failed: {e}")))?
    };

    Ok((automation, walker, root_element))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Walk the UIA tree for the given app and return an `ElementTree` plus a
/// ref→handle cache captured during the walk (used for O(1) ref resolution).
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running.
pub(super) fn snapshot(
    app: &AppTarget,
    window: Option<&WindowTarget>,
    options: &SnapshotOptions,
) -> Result<(ElementTree, RefHandleMap), ForepawError> {
    let (_automation, walker, root_element) = uia_root(app, window)?;

    // Get window bounds from UIA for coordinate display
    let window_bounds = get_element_bounds(&root_element);

    let pruning = TreePruning {
        skip_zero_size: options.skip_zero_size,
        skip_offscreen: options.skip_offscreen,
    };

    let effective_depth = options.max_depth;

    let walk_start = std::time::Instant::now();
    let (root, handle_root) = build_tree(&walker, &root_element, 0, effective_depth, &pruning);
    let walk_ms = walk_start.elapsed().as_secs_f64() * 1000.0;

    let assigner = RefAssigner::new();
    let result = assigner.assign(&root, options.interactive_only);

    // Capture ref→handle from the same walk that produced the tree (interactive
    // nodes only, mirroring `RefAssigner`), so resolve calls are O(1).
    let ref_handles = RefHandleMap(build_ref_handle_map(&root, &handle_root));

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

    let mut tree = ElementTree {
        app: app.display(),
        root: result.root,
        refs: result.refs,
        window_bounds,
        timing,
    };
    tree.enrich();
    Ok((tree, ref_handles))
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
) -> (ElementNode, HandleNode) {
    if depth >= max_depth {
        return (
            ElementNode::new(ElementData::new(Role::Group)),
            HandleNode::default(),
        );
    }

    // Read properties via direct accessors (no VARIANT/SAFEARRAY)
    let role = get_control_type(element);
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let name = get_bstr_property(element, |e| unsafe { e.CurrentName() });
    let bounds = get_element_bounds(element);

    // Retain the UIA handle for every real node (`Clone` = `AddRef`); the
    // ref→handle cache holds one ref per interactive node, released when the
    // map drops. Depth-limit nodes (above) carry no handle.
    let handle = element.clone();

    // Prune zero-size elements (collapsed menus, hidden panels). A pruned
    // interactive leaf keeps its role (so it still gets a ref) and carries its
    // handle, mirroring `RefAssigner`.
    if pruning.skip_zero_size {
        if let Some(b) = &bounds {
            if b.width == 0.0 && b.height == 0.0 && depth > 1 {
                return (
                    ElementNode::new(
                        ElementData::new(role)
                            .with_resolved_name(
                                non_empty(name.as_ref()).map(|n| (n, NameSource::Title)),
                            )
                            .with_bounds(*b),
                    ),
                    HandleNode::leaf(handle),
                );
            }
        }
    }

    // Prune offscreen elements (same lockstep contract as zero-size).
    if pruning.skip_offscreen && depth > 1 && is_offscreen(element) {
        return (
            ElementNode::new(
                ElementData::new(role)
                    .with_resolved_name(non_empty(name.as_ref()).map(|n| (n, NameSource::Title)))
                    .with_bounds_opt(bounds),
            ),
            HandleNode::leaf(handle),
        );
    }

    // Walk children via TreeWalker (depth-first)
    let (children, child_handles) = walk_children(walker, element, depth, max_depth, pruning);

    // Name resolution: CurrentName → CurrentHelpText → first text child
    let (final_name, name_source) = match resolve_name(element, &children, name.as_ref()) {
        Some((n, s)) => (Some(n), Some(s)),
        None => (None, None),
    };

    let node = ElementNode {
        data: ElementData {
            role,
            name: final_name,
            name_source,
            value: None, // TODO: UIA Value pattern for element values
            reference: None,
            bounds,
            bounds_window: None,
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
    };
    (
        node,
        HandleNode {
            handle: Some(handle),
            children: child_handles,
        },
    )
}

/// Walk children using `TreeWalker` GetFirstChild/GetNextSibling. Returns the
/// `ElementNode` children and their parallel `HandleNode`s in lockstep.
fn walk_children(
    walker: &IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    parent_depth: usize,
    max_depth: usize,
    pruning: &TreePruning,
) -> (Vec<ElementNode>, Vec<HandleNode>) {
    let mut children = Vec::new();
    let mut child_handles = Vec::new();

    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let Ok(first_child) = (unsafe { walker.GetFirstChildElement(parent) }) else {
        return (children, child_handles);
    };

    let (node, handles) = build_tree(walker, &first_child, parent_depth + 1, max_depth, pruning);
    children.push(node);
    child_handles.push(handles);

    let mut current = first_child;
    loop {
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        let Ok(next) = (unsafe { walker.GetNextSiblingElement(&current) }) else {
            break;
        };
        let (node, handles) = build_tree(walker, &next, parent_depth + 1, max_depth, pruning);
        children.push(node);
        child_handles.push(handles);
        current = next;
    }

    (children, child_handles)
}

// ---------------------------------------------------------------------------
// Name resolution
// ---------------------------------------------------------------------------

/// Resolve the accessible name, tagging the source.
///
/// Chain: `CurrentName` -> [`NameSource::Title`], `CurrentHelpText` ->
/// [`NameSource::HelpText`], first `StaticText` child's name ->
/// [`NameSource::ChildLabel`].
fn resolve_name(
    element: &IUIAutomationElement,
    children: &[ElementNode],
    current_name: Option<&String>,
) -> Option<(String, NameSource)> {
    // 1. CurrentName
    if let Some(n) = non_empty(current_name) {
        return Some((n, NameSource::Title));
    }

    // 2. CurrentHelpText
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let help = get_bstr_property(element, |e| unsafe { e.CurrentHelpText() });
    if let Some(h) = non_empty(help.as_ref()) {
        return Some((h, NameSource::HelpText));
    }

    // 3. First child that looks like a text label (`StaticText` with a name)
    for child in children {
        if child.data.role == Role::StaticText {
            if let Some(ref name) = child.data.name {
                if !name.is_empty() {
                    return Some((name.clone(), NameSource::ChildLabel));
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
pub(super) fn get_control_type(element: &IUIAutomationElement) -> Role {
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

// ---------------------------------------------------------------------------
// Ref resolution (for action dispatch)
// ---------------------------------------------------------------------------

/// Resolve a ref to its center position, using a retained handle when available.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the element has no bounds.
pub(super) fn resolve_ref_position(
    ref_id: i32,
    app: &AppTarget,
    cached: Option<IUIAutomationElement>,
) -> Result<Point, ForepawError> {
    let bounds = resolve_ref_bounds(ref_id, app, cached)?;
    Ok(bounds.center())
}

/// Resolve a ref to its bounding rect (physical screen px), using a retained
/// handle when available, else a full tree re-walk.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists, or
/// [`ForepawError::ActionFailed`] if the element has no bounds.
pub(super) fn resolve_ref_bounds(
    ref_id: i32,
    app: &AppTarget,
    cached: Option<IUIAutomationElement>,
) -> Result<Rect, ForepawError> {
    let element = resolve_ref_element(ref_id, app, cached)?;
    get_element_bounds(&element)
        .ok_or_else(|| ForepawError::ActionFailed("element has no bounds".into()))
}

/// Resolve a ref to its `IUIAutomationElement`, using a retained handle from
/// the last snapshot when available (O(1)), else a full re-walk.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree.
pub(super) fn resolve_ref_element(
    ref_id: i32,
    app: &AppTarget,
    cached: Option<IUIAutomationElement>,
) -> Result<IUIAutomationElement, ForepawError> {
    if let Some(handle) = cached {
        return Ok(handle);
    }
    resolve_ref_element_rewalk(ref_id, app)
}

/// Fallback re-walk when no retained handle is cached (e.g. resolve before any
/// snapshot on this provider). Best-effort: walks the `ControlView` tree from the
/// app's best window at the default depth, numbering interactive nodes like
/// `RefAssigner`. The cached path is the normal one and is exact.
///
/// This cannot reproduce the pruning a caller's `snapshot` used (depth,
/// zero-size, offscreen): `resolve_ref_*` takes only the ref and app, and
/// `forepaw` is a library, so the caller may have snapshotted with any
/// `SnapshotOptions`. If that snapshot pruned an interactive node that had
/// interactive children, the re-walk (which prunes nothing) numbers them too,
/// shifting subsequent refs. The cache avoids this entirely (same walk that
/// built the tree); when it's absent, treat re-walk resolution as approximate.
fn resolve_ref_element_rewalk(
    ref_id: i32,
    app: &AppTarget,
) -> Result<IUIAutomationElement, ForepawError> {
    let (_automation, walker, root) = uia_root(app, None)?;
    let mut elements: HashMap<i32, IUIAutomationElement> = HashMap::new();
    let mut counter: i32 = 1;
    collect_uia_elements(
        &walker,
        &root,
        0,
        SnapshotOptions::default().max_depth,
        &mut counter,
        &mut elements,
    );
    elements
        .remove(&ref_id)
        .ok_or_else(|| ForepawError::StaleRef(ElementRef::new(ref_id)))
}

/// Walk the UIA tree, collecting `IUIAutomationElement` handles for interactive
/// elements in depth-first order. Must mirror the order used by `RefAssigner`.
fn collect_uia_elements(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    depth: usize,
    max_depth: usize,
    counter: &mut i32,
    elements: &mut HashMap<i32, IUIAutomationElement>,
) {
    if depth >= max_depth {
        return;
    }
    let role = get_control_type(element);
    if role.is_interactive() {
        elements.insert(*counter, element.clone());
        *counter += 1;
    }
    // SAFETY: Win32/WinRT FFI call with valid arguments.
    let Ok(first) = (unsafe { walker.GetFirstChildElement(element) }) else {
        return;
    };
    collect_uia_elements(walker, &first, depth + 1, max_depth, counter, elements);
    let mut current = first;
    loop {
        // SAFETY: Win32/WinRT FFI call with valid arguments.
        let Ok(next) = (unsafe { walker.GetNextSiblingElement(&current) }) else {
            return;
        };
        collect_uia_elements(walker, &next, depth + 1, max_depth, counter, elements);
        current = next;
    }
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
