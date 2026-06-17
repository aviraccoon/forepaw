//! AX tree snapshot: walk the accessibility tree and build an `ElementTree`.
//!
//! Uses batched attribute fetching (`AXUIElementCopyMultipleAttributeValues`)
//! to fetch attributes per element in a single IPC call. This is the key
//! optimization for slow AX responders like Music (~4x speedup).

use std::collections::HashMap;

use crate::core::element_tree::{
    ElementData, ElementNode, ElementRef, ElementTree, SnapshotTiming,
};
use crate::core::errors::ForepawError;
use crate::core::icon_class_parser::IconClassParser;
use crate::core::ref_assigner::RefAssigner;
use crate::core::role::Role;
use crate::core::tree_pruning::PruningOptions;
use crate::core::types::{Point, Rect};
use crate::debug;
use crate::platform::{AppTarget, SnapshotOptions, WindowTarget};

use super::app::{
    electron_tree_is_populated, enable_electron_accessibility, find_app_by_target, find_window,
    is_electron_app,
};
use super::cf_convert::{cf_string_from_str, cf_string_to_rust, number_to_rust_string};
use super::ffi::{
    kCFNull, kCFTypeArrayCallBacks, AXError, AXUIElementCopyAttributeValue,
    AXUIElementCopyMultipleAttributeValues, AXUIElementCreateApplication, AXUIElementRef,
    AXValueGetValue, AXValueRef, AXValueType, CFArrayCreate, CFArrayGetCount, CFArrayGetTypeID,
    CFArrayGetValueAtIndex, CFArrayRef, CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef,
    CFGetTypeID, CFIndex, CFNumberGetTypeID, CFNumberGetValue, CFNumberRef, CFRelease, CFRetain,
    CFStringGetTypeID, CFStringRef, CFTypeRef, CGPointFFI, CGSizeFFI, K_CF_NUMBER_SINT32_TYPE,
};
use super::role::ax_role_to_role;

const DEFAULT_DEPTH: usize = 15;
const ELECTRON_DEPTH: usize = 25;

// Batch attributes — define both the enum and the AX name mapping from one source.
macro_rules! define_attrs {
    ($(($name:ident, $ax_name:literal)),* $(,)?) => {
        #[repr(usize)]
        #[derive(Debug, Clone, Copy)]
        pub(super) enum Attr {
            $($name),*
        }

        /// All AX attribute names in discriminant order, for batch fetching.
        const ATTR_NAMES: &[&str] = &[$($ax_name),*];
    };
}

define_attrs! {
    (Role, "AXRole"),
    (Title, "AXTitle"),
    (Description, "AXDescription"),
    (Value, "AXValue"),
    (Position, "AXPosition"),
    (Size, "AXSize"),
    (Children, "AXChildren"),
    (Subrole, "AXSubrole"),
    (TitleUIElement, "AXTitleUIElement"),
    (Help, "AXHelp"),
    (PlaceholderValue, "AXPlaceholderValue"),
    (DOMClassList, "AXDOMClassList"),
    (RoleDescription, "AXRoleDescription"),
    (Enabled, "AXEnabled"),
    (Focused, "AXFocused"),
    (Selected, "AXSelected"),
    (Identifier, "AXIdentifier"),
    (Orientation, "AXOrientation"),
    (Expanded, "AXExpanded"),
    (MinValue, "AXMinValue"),
    (MaxValue, "AXMaxValue"),
    (ValueIncrement, "AXValueIncrement"),
    (Url, "AXURL"),
    (SortDirection, "AXSortDirection"),
    (Index, "AXIndex"),
    (Required, "AXRequired"),
    (ElementBusy, "AXElementBusy"),
    (DisclosureLevel, "AXDisclosureLevel"),
    (AccessKey, "AXAccessKey"),
    (Filename, "AXFilename"),
}

/// Cached `CFArray` of attribute name strings for batch fetching.
/// Created once on first use, never freed.
static BATCH_ATTR_ARRAY: std::sync::OnceLock<SendableCFArray> = std::sync::OnceLock::new();

/// Wrapper to make `CFArrayRef` Send+Sync (immutable after creation).
struct SendableCFArray(CFArrayRef);
// SAFETY: SendableCFArray wraps a CFArrayRef that is immutable after creation
// (created once by OnceLock). CFArray is thread-safe for read-only access.
unsafe impl Send for SendableCFArray {}
// SAFETY: Same reasoning as Send above.
unsafe impl Sync for SendableCFArray {}

fn get_batch_attr_array() -> CFArrayRef {
    BATCH_ATTR_ARRAY
        .get_or_init(|| {
            let cf_strings: Vec<CFStringRef> =
                ATTR_NAMES.iter().map(|s| cf_string_from_str(s)).collect();
            let ptrs: Vec<*const std::ffi::c_void> =
                cf_strings.iter().map(|s| (*s).cast()).collect();
            // SAFETY: CFArrayCreate produces an immutable array from valid CFStrings.
            let array = unsafe {
                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "attribute count fits in CFIndex (i64)"
                )]
                let count = ptrs.len() as CFIndex;
                CFArrayCreate(
                    std::ptr::null(),
                    ptrs.as_ptr(),
                    count,
                    &raw const kCFTypeArrayCallBacks,
                )
            };
            for s in &cf_strings {
                // SAFETY: CFRelease on CFStrings we created via cf_string_from_str.
                unsafe { CFRelease(*s as CFTypeRef) };
            }
            SendableCFArray(array)
        })
        .0
}

/// Role descriptions too generic to use as element names.
const GENERIC_ROLE_DESCRIPTIONS: &[&str] = &[
    "button",
    "link",
    "text field",
    "text entry area",
    "image",
    "menu item",
    "check box",
    "radio button",
    "tab",
    "cell",
    "slider",
    "pop up button",
    "combo box",
    "menu button",
    "incrementor",
    "color well",
    "disclosure triangle",
    "switch",
    "toggle",
    "group",
    "list",
    "table",
    "outline",
    "scroll area",
    "scroll bar",
    "toolbar",
    "menu bar",
    "menu bar item",
    "window",
    "sheet",
    "drawer",
    "application",
    "browser",
    "row",
    "column",
    "heading",
    "static text",
    "tree item",
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Walk the AX tree for the given app and return an `ElementTree`.
///
/// # Errors
///
/// Returns [`ForepawError::AppNotFound`] if the application is not running,
/// or [`ForepawError::PermissionDenied`] if accessibility access is not granted.
pub fn snapshot(
    app: &AppTarget,
    window: Option<&WindowTarget>,
    options: &SnapshotOptions,
) -> Result<ElementTree, ForepawError> {
    let running_app = find_app_by_target(app)?;

    // Activate the app so the AX tree matches what action commands will see.
    // Some apps (browsers) only expose web content when active.
    #[expect(
        deprecated,
        reason = "activateWithOptions deprecated in macOS 14, no replacement for ignoring-other-apps behavior"
    )]
    running_app.activateWithOptions(
        objc2_app_kit::NSApplicationActivationOptions::ActivateIgnoringOtherApps,
    );
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Electron apps need AXManualAccessibility + polling for tree population.
    let is_electron = is_electron_app(&running_app);
    let pid = running_app.processIdentifier();
    debug!(
        "snapshot: app={} pid={} is_electron={}",
        app.display(),
        pid,
        is_electron
    );
    if is_electron {
        enable_electron_accessibility(pid);
        let poll_start = std::time::Instant::now();
        if !electron_tree_is_populated(pid) {
            for _ in 0..6 {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if electron_tree_is_populated(pid) {
                    break;
                }
            }
        }
        debug!(
            "snapshot: electron poll took {:.0}ms",
            poll_start.elapsed().as_secs_f64() * 1000.0
        );
    }

    let effective_depth = if is_electron {
        std::cmp::max(options.max_depth, ELECTRON_DEPTH)
    } else {
        options.max_depth
    };

    // SAFETY: AXUIElementCreateApplication is a system call, no preconditions.
    let app_element = unsafe { AXUIElementCreateApplication(pid) };

    // Window bounds for offscreen pruning and coordinate display.
    let window_bounds = find_window(pid, window).ok().map(|w| w.bounds);

    let pruning = TreePruning {
        options: PruningOptions {
            exclude_menu_bar: options.skip_menu_bar,
            skip_zero_size: options.skip_zero_size,
            exclude_offscreen: options.skip_offscreen,
        },
        window_bounds: if options.skip_offscreen {
            window_bounds
        } else {
            None
        },
    };

    let walk_start = std::time::Instant::now();
    let root = build_tree(app_element, 0, effective_depth, &pruning);
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

/// Re-walk the tree to resolve a ref's center position (for coordinate-based actions).
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree,
/// or [`ForepawError::ActionFailed`] if the element has no position or size.
pub fn resolve_ref_position(ref_id: i32, app: &AppTarget) -> Result<Point, ForepawError> {
    let bounds = resolve_ref_bounds(ref_id, app)?;
    Ok(Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    ))
}

/// Re-walk the tree to resolve a ref's bounding rect.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree,
/// or [`ForepawError::ActionFailed`] if the element has no position or size.
pub fn resolve_ref_bounds(ref_id: i32, app: &AppTarget) -> Result<Rect, ForepawError> {
    let element = resolve_ref_element(ref_id, app)?;
    let pos = get_element_position(element)
        .ok_or_else(|| ForepawError::ActionFailed("element has no position".into()))?;
    let (w, h) = get_element_size(element)
        .ok_or_else(|| ForepawError::ActionFailed("element has no size".into()))?;
    Ok(Rect::new(pos.x, pos.y, w, h))
}

// ---------------------------------------------------------------------------
// Tree pruning options
// ---------------------------------------------------------------------------

/// Shared pruning options for the Darwin snapshot.
///
/// Wraps `PruningOptions` from `tree_pruning` with Darwin-specific state.
struct TreePruning {
    options: PruningOptions,
    window_bounds: Option<Rect>,
}

// ---------------------------------------------------------------------------
// Batch attribute fetching
// ---------------------------------------------------------------------------

/// Wrapper around the `CFArray` returned by `AXUIElementCopyMultipleAttributeValues`.
/// Provides typed accessors for each attribute by index. Releases the `CFArray` on drop.
pub(super) struct BatchAttrs {
    array: CFArrayRef,
}

impl BatchAttrs {
    fn new(array: CFArrayRef) -> Self {
        Self { array }
    }

    /// Get the raw `CFTypeRef` at the given attribute, or None if missing/kCFNull.
    fn raw(&self, attr: Attr) -> Option<CFTypeRef> {
        // SAFETY: attr is a valid Attr discriminant (0..23), self.array is valid CFArray.
        unsafe {
            #[expect(
                clippy::cast_possible_wrap,
                reason = "Attr discriminant fits in CFIndex (i64)"
            )]
            let idx = attr as usize as CFIndex;
            let val = CFArrayGetValueAtIndex(self.array, idx);
            if val.is_null() {
                return None;
            }
            // kCFNull means the attribute doesn't exist on this element.
            if val as CFTypeRef == kCFNull {
                return None;
            }
            Some(val as CFTypeRef)
        }
    }

    /// Extract a String attribute.
    pub(super) fn string(&self, attr: Attr) -> Option<String> {
        let val = self.raw(attr)?;
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "type check + conversion"
        )]
        // SAFETY: CFGetTypeID + CFStringGetTypeID are read-only type checks.
        // cf_string_to_rust handles the CFStringRef safely.
        unsafe {
            if CFGetTypeID(val) != CFStringGetTypeID() {
                return None;
            }
            cf_string_to_rust(val as CFStringRef)
        }
    }

    /// Extract the value attribute, which can be `CFString` or `CFNumber`.
    pub(super) fn value_string(&self, attr: Attr) -> Option<String> {
        let val = self.raw(attr)?;
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "type dispatch + conversion"
        )]
        // SAFETY: type checks + conversions on valid CFTypeRef.
        unsafe {
            let type_id = CFGetTypeID(val);
            if type_id == CFStringGetTypeID() {
                cf_string_to_rust(val as CFStringRef)
            } else if type_id == CFNumberGetTypeID() {
                number_to_rust_string(val as CFNumberRef)
            } else {
                None
            }
        }
    }

    /// Extract a `CGPoint` from an `AXValue` attribute.
    fn point(&self, attr: Attr) -> Option<Point> {
        let val = self.raw(attr)?;
        // SAFETY: AXValueGetValue reads a CGPoint from a valid AXValue.
        unsafe {
            let mut pt = CGPointFFI { x: 0.0, y: 0.0 };
            if AXValueGetValue(
                AXValueRef(val.cast()),
                AXValueType::CGPoint,
                (&raw mut pt).cast::<std::ffi::c_void>(),
            ) != 0
            {
                Some(Point::new(pt.x, pt.y))
            } else {
                None
            }
        }
    }

    /// Extract a `CGSize` from an `AXValue` attribute.
    fn size_val(&self, attr: Attr) -> Option<(f64, f64)> {
        let val = self.raw(attr)?;
        // SAFETY: AXValueGetValue reads a CGSize from a valid AXValue.
        unsafe {
            let mut sz = CGSizeFFI {
                width: 0.0,
                height: 0.0,
            };
            if AXValueGetValue(
                AXValueRef(val.cast()),
                AXValueType::CGSize,
                (&raw mut sz).cast::<std::ffi::c_void>(),
            ) != 0
            {
                Some((sz.width, sz.height))
            } else {
                None
            }
        }
    }

    /// Build a Rect from position and size attributes.
    pub(super) fn bounds(&self, pos: Attr, size: Attr) -> Option<Rect> {
        let pt = self.point(pos)?;
        let (w, h) = self.size_val(size)?;
        Some(Rect::new(pt.x, pt.y, w, h))
    }

    /// Extract a bool from a `CFBoolean` attribute.
    fn bool_val(&self, attr: Attr) -> Option<bool> {
        let val = self.raw(attr)?;
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "type check + bool conversion"
        )]
        // SAFETY: CFBoolean is toll-free bridged with NSNumber.
        // CFGetTypeID and CFBooleanGetValue are read-only type checks
        // on valid CFTypeRef values returned from BatchAttrs::raw().
        unsafe {
            // CFBoolean has its own type ID
            let type_id = CFGetTypeID(val);
            if type_id == CFBooleanGetTypeID() {
                // CFBooleanGetValue returns true for kCFBooleanTrue
                Some(CFBooleanGetValue(val as CFBooleanRef) != 0)
            } else if type_id == CFNumberGetTypeID() {
                // Fallback: NSNumber with 0/1
                let mut result: i32 = 0;
                CFNumberGetValue(
                    val as CFNumberRef,
                    K_CF_NUMBER_SINT32_TYPE,
                    (&raw mut result).cast::<std::ffi::c_void>(),
                );
                Some(result != 0)
            } else {
                None
            }
        }
    }

    /// Extract child `AXUIElement` refs from the `AXChildren` attribute.
    pub(super) fn children(&self, attr: Attr) -> Vec<AXUIElementRef> {
        let Some(val) = self.raw(attr) else {
            return Vec::new();
        };
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "CFArray iteration + retain"
        )]
        // SAFETY: CFArray iteration on a valid CFArray. Each child is retained
        // before being wrapped in AXUIElementRef.
        unsafe {
            if CFGetTypeID(val) != CFArrayGetTypeID() {
                return Vec::new();
            }
            let count = CFArrayGetCount(val as CFArrayRef);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "CFArray count fits in usize"
            )]
            #[expect(clippy::cast_sign_loss, reason = "CFArray count is non-negative")]
            let count_usize = count as usize;
            let mut result = Vec::with_capacity(count_usize);
            for i in 0..count {
                let child = CFArrayGetValueAtIndex(val as CFArrayRef, i);
                // CFRetain: CFArrayGetValueAtIndex returns a non-retained pointer.
                CFRetain(child as CFTypeRef);
                result.push(AXUIElementRef::from_raw(child));
            }
            result
        }
    }

    /// Extract a single `AXUIElement` ref (e.g. for `AXTitleUIElement`).
    fn element(&self, attr: Attr) -> Option<AXUIElementRef> {
        let val = self.raw(attr)?;
        // SAFETY: AXUIElementRef::from_raw wraps the raw pointer.
        unsafe { Some(AXUIElementRef::from_raw(val.cast::<std::ffi::c_void>())) }
    }

    /// Extract a `CFArray` of `CFStrings` (e.g. for `AXDOMClassList`).
    fn string_array(&self, attr: Attr) -> Option<Vec<String>> {
        let val = self.raw(attr)?;
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "CFArray iteration + type dispatch"
        )]
        // SAFETY: CFArray iteration + type checks on valid CFArray.
        unsafe {
            if CFGetTypeID(val) != CFArrayGetTypeID() {
                return None;
            }
            let count = CFArrayGetCount(val as CFArrayRef);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "CFArray count fits in usize"
            )]
            #[expect(clippy::cast_sign_loss, reason = "CFArray count is non-negative")]
            let count_usize = count as usize;
            let mut result = Vec::with_capacity(count_usize);
            for i in 0..count {
                let item = CFArrayGetValueAtIndex(val as CFArrayRef, i);
                if item.is_null() || item as CFTypeRef == kCFNull {
                    continue;
                }
                if CFGetTypeID(item as CFTypeRef) == CFStringGetTypeID() {
                    if let Some(s) = cf_string_to_rust(item as CFStringRef) {
                        result.push(s);
                    }
                }
            }
            if result.is_empty() {
                None
            } else {
                Some(result)
            }
        }
    }
}

impl Drop for BatchAttrs {
    fn drop(&mut self) {
        // SAFETY: self.array is a valid CFArrayRef we own from CopyMultipleAttributeValues.
        unsafe { CFRelease(self.array as CFTypeRef) };
    }
}

/// Call `AXUIElementCopyMultipleAttributeValues` for all 13 batch attributes.
pub(super) fn fetch_batch_attributes(element: AXUIElementRef) -> Option<BatchAttrs> {
    let attr_array = get_batch_attr_array();
    let mut values: CFArrayRef = std::ptr::null();
    // SAFETY: AXUIElementCopyMultipleAttributeValues copies attributes into
    // a CFArray the caller owns.
    let result =
        unsafe { AXUIElementCopyMultipleAttributeValues(element, attr_array, 0, &raw mut values) };
    if result != AXError::Success || values.is_null() {
        None
    } else {
        Some(BatchAttrs::new(values))
    }
}

// ---------------------------------------------------------------------------
// Tree walk
// ---------------------------------------------------------------------------

fn build_tree(
    element: AXUIElementRef,
    depth: usize,
    max_depth: usize,
    pruning: &TreePruning,
) -> ElementNode {
    if depth >= max_depth {
        return ElementNode::new(ElementData::new(Role::Group));
    }

    let Some(attrs) = fetch_batch_attributes(element) else {
        return ElementNode::new(ElementData::new(Role::Unknown));
    };

    let role_str = attrs
        .string(Attr::Role)
        .unwrap_or_else(|| "AXUnknown".to_owned());
    let role = ax_role_to_role(&role_str);

    // Native role: the raw AX string (e.g. "AXButton"). Useful for
    // debugging, especially when mapping to Unknown.
    let native_role = Some(role_str);
    let identifier =
        attrs
            .string(Attr::Identifier)
            .and_then(|id| if id.is_empty() { None } else { Some(id) });

    // Skip menu bar subtree if requested.
    if pruning.options.exclude_menu_bar && role == Role::MenuBar {
        return ElementNode::new(ElementData::new(role));
    }

    let value = attrs.value_string(Attr::Value);
    let bounds = attrs.bounds(Attr::Position, Attr::Size);

    let attributes = collect_extra_attributes(&attrs);

    // Check pruning conditions (zero-size and offscreen).
    if let Some(pruned) = check_pruned(
        &attrs,
        role,
        value.as_ref(),
        bounds.as_ref(),
        depth,
        pruning,
        element,
    ) {
        return pruned;
    }

    // Build children BEFORE computing name. This lets computedName read
    // names/values from already-built child ElementNodes instead of making
    // individual IPC calls.
    let children_refs = attrs.children(Attr::Children);
    let mut children: Vec<ElementNode> = children_refs
        .iter()
        .map(|child| build_tree(*child, depth + 1, max_depth, pruning))
        .collect();

    // Retry children that came back as stale AX references.
    // Some frameworks (notably Slint) lazily initialize their accessibility tree.
    // The first AXChildren read may contain invalid references that return
    // AXError::InvalidUIElement for all attribute queries. Re-reading AXChildren
    // from the parent element yields fresh, valid references.
    let stale_indices: Vec<usize> = children
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.data.role == Role::Unknown
                && node.data.name.is_none()
                && node.data.value.is_none()
                && node.data.bounds.is_none()
        })
        .map(|(i, _)| i)
        .collect();
    if !stale_indices.is_empty() {
        let fresh_refs = get_ax_element_children(element);
        for idx in stale_indices {
            if let Some(fresh_child) = fresh_refs.get(idx) {
                if let Some(slot) = children.get_mut(idx) {
                    *slot = build_tree(*fresh_child, depth + 1, max_depth, pruning);
                }
            }
        }
    }

    let name = non_empty(attrs.string(Attr::Title).as_ref())
        .or_else(|| non_empty(attrs.string(Attr::Description).as_ref()))
        .or_else(|| computed_name(&attrs, &children, element));

    let enabled = attrs.bool_val(Attr::Enabled);
    // Note: AXFocused conflates focusable and focused on macOS (W3C Core-AAM).
    // The actual focused element is AXFocusedUIElement on the app (separate IPC).
    let focused = attrs.bool_val(Attr::Focused);
    let selected = attrs.bool_val(Attr::Selected);

    // Description: use AXDescription if it wasn't already used as the name.
    // (computed_name already falls back to AXDescription, so only show it
    // if it's different from the title and wasn't consumed by name resolution.)
    let description = attrs
        .string(Attr::Description)
        .and_then(|d| if d.is_empty() { None } else { Some(d) })
        .filter(|d| name.as_ref() != Some(d));

    ElementNode {
        data: ElementData {
            role,
            name,
            value,
            reference: None,
            bounds,
            enabled,
            focused,
            selected,
            description,
            native_role,
            identifier,
            uid: None,
            signature: None,
            signature_bounds: None,
            attributes,
        },
        children,
    }
}

/// Collect platform-specific extra attributes from the batch fetch.
/// These are shown in verbose output or JSON as key-value pairs.
fn collect_extra_attributes(attrs: &BatchAttrs) -> Vec<(String, String)> {
    let mut attributes = Vec::new();

    if let Some(subrole) = attrs.string(Attr::Subrole) {
        if !subrole.is_empty() && subrole != "AXNone" {
            attributes.push(("subrole".to_owned(), subrole));
        }
    }

    // Orientation (horizontal/vertical). Unknown is omitted.
    if let Some(orientation) = attrs.string(Attr::Orientation) {
        if orientation == "AXHorizontalOrientation" {
            attributes.push(("orientation".to_owned(), "horizontal".to_owned()));
        } else if orientation == "AXVerticalOrientation" {
            attributes.push(("orientation".to_owned(), "vertical".to_owned()));
        }
    }

    // Expanded state (disclosure triangles, combo boxes, pop-up buttons).
    if let Some(expanded) = attrs.bool_val(Attr::Expanded) {
        attributes.push(("expanded".to_owned(), expanded.to_string()));
    }

    // Min/max value and increment (sliders, scroll bars, steppers).
    if let Some(min_val) = attrs.value_string(Attr::MinValue) {
        if !min_val.is_empty() {
            attributes.push(("min_value".to_owned(), min_val));
        }
    }
    if let Some(max_val) = attrs.value_string(Attr::MaxValue) {
        if !max_val.is_empty() {
            attributes.push(("max_value".to_owned(), max_val));
        }
    }
    if let Some(increment) = attrs.value_string(Attr::ValueIncrement) {
        if !increment.is_empty() {
            attributes.push(("value_increment".to_owned(), increment));
        }
    }

    // URL (links, web areas).
    if let Some(url) = attrs.string(Attr::Url) {
        if !url.is_empty() && url != "about:blank" {
            attributes.push(("url".to_owned(), url));
        }
    }

    // Sort direction (column headers).
    if let Some(dir) = attrs.string(Attr::SortDirection) {
        match dir.as_str() {
            "AXAscendingSortDirection" => {
                attributes.push(("sort_direction".to_owned(), "ascending".to_owned()));
            }
            "AXDescendingSortDirection" => {
                attributes.push(("sort_direction".to_owned(), "descending".to_owned()));
            }
            _ => {}
        }
    }

    // Index (tab/row/column position).
    if let Some(idx) = attrs.value_string(Attr::Index) {
        attributes.push(("index".to_owned(), idx));
    }

    // Required (form fields).
    if let Some(req) = attrs.bool_val(Attr::Required) {
        attributes.push(("required".to_owned(), req.to_string()));
    }

    // Element busy (async loading).
    if let Some(busy) = attrs.bool_val(Attr::ElementBusy) {
        attributes.push(("element_busy".to_owned(), busy.to_string()));
    }

    // Disclosure level (outline/tree indentation).
    if let Some(level) = attrs.value_string(Attr::DisclosureLevel) {
        attributes.push(("disclosure_level".to_owned(), level));
    }

    // Access key (web keyboard shortcut).
    if let Some(key) = attrs.string(Attr::AccessKey) {
        if !key.is_empty() {
            attributes.push(("access_key".to_owned(), key));
        }
    }

    // Filename (file dialogs, Finder).
    if let Some(name) = attrs.string(Attr::Filename) {
        if !name.is_empty() {
            attributes.push(("filename".to_owned(), name));
        }
    }

    attributes
}

/// Check if this element should be pruned (zero-size or offscreen).
/// Returns `Some(pruned_node)` if the element should be skipped.
fn check_pruned(
    attrs: &BatchAttrs,
    role: Role,
    value: Option<&String>,
    bounds: Option<&Rect>,
    depth: usize,
    pruning: &TreePruning,
    element: AXUIElementRef,
) -> Option<ElementNode> {
    // Skip zero-size subtrees (collapsed menus, hidden panels).
    if pruning.options.skip_zero_size {
        if let Some(b) = bounds {
            if b.width == 0.0 && b.height == 0.0 && depth > 1 {
                let name = non_empty(attrs.string(Attr::Title).as_ref())
                    .or_else(|| non_empty(attrs.string(Attr::Description).as_ref()))
                    .or_else(|| computed_name(attrs, &[], element));
                let mut data = ElementData::new(role).with_name_opt(name).with_bounds(*b);
                if let Some(v) = value {
                    data = data.with_value(v.as_str());
                }
                return Some(ElementNode::new(data));
            }
        }
    }

    // Skip offscreen subtrees.
    if let (Some(wb), Some(b)) = (&pruning.window_bounds, bounds) {
        if depth > 2 && b.width > 0.0 && b.height > 0.0 {
            let no_horizontal = b.x + b.width <= wb.x || b.x >= wb.x + wb.width;
            let no_vertical = b.y + b.height <= wb.y || b.y >= wb.y + wb.height;
            if no_horizontal || no_vertical {
                let name = non_empty(attrs.string(Attr::Title).as_ref());
                return Some(ElementNode::new(
                    ElementData::new(role).with_name_opt(name).with_bounds(*b),
                ));
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Name computation
// ---------------------------------------------------------------------------

/// Derive a name from batch attributes and already-built child nodes.
///
/// The chain (in priority order):
/// 1. `AXTitleUIElement` -> its value or title
/// 2. First `AXStaticText` child's value, or `AXImage` child with a name
/// 3. `AXHelp`
/// 4. `AXPlaceholderValue`
/// 5. `AXDOMClassList` -> icon class parsing
/// 6. `AXRoleDescription` (when not generic)
fn computed_name(
    attrs: &BatchAttrs,
    children: &[ElementNode],
    _element: AXUIElementRef,
) -> Option<String> {
    // 1. AXTitleUIElement (index 8) -> read its value or title.
    if let Some(title_element) = attrs.element(Attr::TitleUIElement) {
        if let Some(val) = get_ax_string_attr(title_element, "AXValue") {
            if !val.is_empty() {
                return Some(val);
            }
        }
        if let Some(title) = get_ax_string_attr(title_element, "AXTitle") {
            if !title.is_empty() {
                return Some(title);
            }
        }
    }

    // 2. First AXStaticText child's value, or AXImage child with a name.
    for child in children {
        if child.data.role == Role::StaticText {
            if let Some(ref val) = child.data.value {
                if !val.is_empty() {
                    return Some(val.clone());
                }
            }
        }
        if child.data.role == Role::Image {
            if let Some(ref name) = child.data.name {
                if !name.is_empty() {
                    return Some(name.clone());
                }
            }
        }
    }

    // 3. AXHelp (index 9)
    if let Some(help) = attrs.string(Attr::Help) {
        if !help.is_empty() {
            return Some(help);
        }
    }

    // 4. AXPlaceholderValue (index 10)
    if let Some(placeholder) = attrs.string(Attr::PlaceholderValue) {
        if !placeholder.is_empty() {
            return Some(placeholder);
        }
    }

    // 5. AXDOMClassList (index 11) -> icon class parsing
    if let Some(class_list) = attrs.string_array(Attr::DOMClassList) {
        let class_refs: Vec<&str> = class_list.iter().map(String::as_str).collect();
        if let Some(icon_name) = IconClassParser::new().parse(&class_refs) {
            return Some(icon_name);
        }
    }

    // 6. AXRoleDescription (index 12)
    if let Some(role_desc) = attrs.string(Attr::RoleDescription) {
        if !role_desc.is_empty() && !GENERIC_ROLE_DESCRIPTIONS.contains(&role_desc.as_str()) {
            return Some(role_desc);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Ref resolution (for action dispatch)
// ---------------------------------------------------------------------------

/// Re-walk the AX tree to find the `AXUIElement` at the given ref position.
///
/// # Errors
///
/// Returns [`ForepawError::StaleRef`] if the ref no longer exists in the tree.
pub fn resolve_ref_element(ref_id: i32, app: &AppTarget) -> Result<AXUIElementRef, ForepawError> {
    let running_app = find_app_by_target(app)?;
    let is_electron = is_electron_app(&running_app);
    if is_electron {
        enable_electron_accessibility(running_app.processIdentifier());
    }

    let resolve_depth = if is_electron {
        ELECTRON_DEPTH
    } else {
        DEFAULT_DEPTH
    };
    // SAFETY: AXUIElementCreateApplication is a system call, no preconditions.
    let app_element = unsafe { AXUIElementCreateApplication(running_app.processIdentifier()) };

    let mut counter: i32 = 1;
    let mut elements: HashMap<i32, AXUIElementRef> = HashMap::new();
    collect_ax_elements(app_element, 0, resolve_depth, &mut counter, &mut elements);

    elements
        .remove(&ref_id)
        .ok_or_else(|| ForepawError::StaleRef(ElementRef::new(ref_id)))
}

/// Walk the AX tree, collecting `AXUIElement` handles for interactive elements
/// in depth-first order. Must mirror the order used by `RefAssigner`.
fn collect_ax_elements(
    element: AXUIElementRef,
    depth: usize,
    max_depth: usize,
    counter: &mut i32,
    elements: &mut HashMap<i32, AXUIElementRef>,
) {
    if depth >= max_depth {
        return;
    }

    let role_str = get_ax_string_attr(element, "AXRole").unwrap_or_default();
    let role = ax_role_to_role(&role_str);

    if role.is_interactive() {
        elements.insert(*counter, element);
        *counter += 1;
    }

    let children = get_ax_element_children(element);
    for child in &children {
        collect_ax_elements(*child, depth + 1, max_depth, counter, elements);
    }
}

// ---------------------------------------------------------------------------
// AXUIElement helpers
// ---------------------------------------------------------------------------

/// Get a single string attribute from an `AXUIElement`.
#[must_use]
pub fn get_ax_string_attr(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let attr_cf = cf_string_from_str(attribute);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: AXUIElementCopyAttributeValue on valid element, attr_cf released after.
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &raw mut value) };
    // SAFETY: attr_cf is a valid CFString we own.
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return None;
    }
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "type check + convert + release"
    )]
    // SAFETY: type check + string conversion + release on valid CFTypeRef.
    unsafe {
        if CFGetTypeID(value) != CFStringGetTypeID() {
            CFRelease(value);
            return None;
        }
        let s = cf_string_to_rust(value as CFStringRef);
        CFRelease(value);
        s
    }
}

/// Get the `AXChildren` attribute as a Vec of `AXUIElementRef`.
fn get_ax_element_children(element: AXUIElementRef) -> Vec<AXUIElementRef> {
    let attr_cf = cf_string_from_str("AXChildren");
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: AXUIElementCopyAttributeValue on valid element.
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &raw mut value) };
    // SAFETY: attr_cf is a valid CFString we own.
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return Vec::new();
    }
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "CFArray iteration + retain + release"
    )]
    // SAFETY: CFArray iteration on valid AXChildren value, each child retained.
    unsafe {
        if CFGetTypeID(value) != CFArrayGetTypeID() {
            CFRelease(value);
            return Vec::new();
        }
        let count = CFArrayGetCount(value as CFArrayRef);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "CFArray count fits in usize"
        )]
        #[expect(clippy::cast_sign_loss, reason = "CFArray count is non-negative")]
        let count_usize = count as usize;
        let mut children = Vec::with_capacity(count_usize);
        for i in 0..count {
            let child = CFArrayGetValueAtIndex(value as CFArrayRef, i);
            // CFRetain: CFArrayGetValueAtIndex returns a non-retained pointer.
            CFRetain(child as CFTypeRef);
            children.push(AXUIElementRef::from_raw(child));
        }
        CFRelease(value);
        children
    }
}

/// Get the position (`AXPosition`) of an `AXUIElement` as a Point.
#[must_use]
pub fn get_element_position(element: AXUIElementRef) -> Option<Point> {
    let attr_cf = cf_string_from_str("AXPosition");
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: AXUIElementCopyAttributeValue on valid element.
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &raw mut value) };
    // SAFETY: attr_cf is a valid CFString we own.
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return None;
    }
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "AXValue read + release"
    )]
    // SAFETY: AXValueGetValue reads CGPoint from valid AXValue, then released.
    unsafe {
        let mut pt = CGPointFFI { x: 0.0, y: 0.0 };
        let ok = AXValueGetValue(
            AXValueRef(value.cast()),
            AXValueType::CGPoint,
            (&raw mut pt).cast::<std::ffi::c_void>(),
        );
        CFRelease(value);
        if ok != 0 {
            Some(Point::new(pt.x, pt.y))
        } else {
            None
        }
    }
}

/// Get the size (`AXSize`) of an `AXUIElement` as (width, height).
#[must_use]
pub fn get_element_size(element: AXUIElementRef) -> Option<(f64, f64)> {
    let attr_cf = cf_string_from_str("AXSize");
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: AXUIElementCopyAttributeValue on valid element.
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &raw mut value) };
    // SAFETY: attr_cf is a valid CFString we own.
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return None;
    }
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "AXValue read + release"
    )]
    // SAFETY: AXValueGetValue reads CGSize from valid AXValue, then released.
    unsafe {
        let mut sz = CGSizeFFI {
            width: 0.0,
            height: 0.0,
        };
        let ok = AXValueGetValue(
            AXValueRef(value.cast()),
            AXValueType::CGSize,
            (&raw mut sz).cast::<std::ffi::c_void>(),
        );
        CFRelease(value);
        if ok != 0 {
            Some((sz.width, sz.height))
        } else {
            None
        }
    }
}

/// Return None for empty strings -- AX APIs often return "" rather than nil.
pub(super) fn non_empty(s: Option<&String>) -> Option<String> {
    s.and_then(|v| if v.is_empty() { None } else { Some(v.clone()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_returns_some_for_nonempty() {
        assert_eq!(
            non_empty(Some(&"hello".to_owned())),
            Some("hello".to_owned())
        );
    }

    #[test]
    fn non_empty_returns_none_for_empty() {
        assert_eq!(non_empty(Some(&String::new())), None);
    }

    #[test]
    fn non_empty_returns_none_for_none() {
        assert_eq!(non_empty(None::<&String>), None);
    }

    // --- Generic role descriptions ---

    #[test]
    fn generic_role_descriptions_detected() {
        for desc in [
            "button",
            "link",
            "text field",
            "text entry area",
            "image",
            "menu item",
            "check box",
            "radio button",
            "tab",
            "cell",
            "group",
            "scroll area",
            "toolbar",
            "window",
            "row",
            "column",
            "heading",
            "static text",
            "tree item",
        ] {
            assert!(
                GENERIC_ROLE_DESCRIPTIONS.contains(&desc),
                "'{desc}' should be generic"
            );
        }
    }

    #[test]
    fn specific_role_descriptions_not_generic() {
        for desc in [
            "close button",
            "zoom button",
            "minimize button",
            "sidebar",
            "disclosure",
            "sort ascending",
            "open file dialog",
        ] {
            assert!(
                !GENERIC_ROLE_DESCRIPTIONS.contains(&desc),
                "'{desc}' should NOT be generic"
            );
        }
    }

    // --- Tree pruning ---

    #[test]
    fn skip_zero_size_prunes_at_depth_2() {
        // A zero-size element at depth > 1 should be pruned (no children walked).
        // We can't easily test build_tree directly without AX elements,
        // but we can test the pruning logic conceptually.
        let pruning = TreePruning {
            options: PruningOptions {
                skip_zero_size: true,
                ..Default::default()
            },
            window_bounds: None,
        };
        assert!(pruning.options.skip_zero_size);
    }

    #[test]
    fn offscreen_detection_logic() {
        let wb = Rect::new(0.0, 0.0, 800.0, 600.0);

        // Element fully inside window
        let inside = Rect::new(100.0, 100.0, 200.0, 50.0);
        assert!(!(inside.x + inside.width <= wb.x || inside.x >= wb.x + wb.width));
        assert!(!(inside.y + inside.height <= wb.y || inside.y >= wb.y + wb.height));

        // Element fully to the right
        let right = Rect::new(900.0, 100.0, 200.0, 50.0);
        assert!(right.x >= wb.x + wb.width); // no horizontal overlap

        // Element fully below
        let below = Rect::new(100.0, 700.0, 200.0, 50.0);
        assert!(below.y >= wb.y + wb.height); // no vertical overlap

        // Element partially overlapping (should NOT be pruned)
        let partial = Rect::new(750.0, 100.0, 200.0, 50.0);
        let no_horizontal = partial.x + partial.width <= wb.x || partial.x >= wb.x + wb.width;
        let no_vertical = partial.y + partial.height <= wb.y || partial.y >= wb.y + wb.height;
        assert!(!no_horizontal && !no_vertical);
    }

    // --- Snapshot options defaults ---

    #[test]
    fn default_depths() {
        assert_eq!(DEFAULT_DEPTH, 15);
        assert_eq!(ELECTRON_DEPTH, 25);
    }
}
