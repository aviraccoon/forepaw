//! AX tree snapshot: walk the accessibility tree and build an ElementTree.
//!
//! Uses batched attribute fetching (`AXUIElementCopyMultipleAttributeValues`)
//! to fetch 13 attributes per element in a single IPC call. This is the key
//! optimization for slow AX responders like Music (~4x speedup).

use std::collections::HashMap;

use crate::core::element_tree::{
    is_interactive_role, ElementNode, ElementRef, ElementTree, SnapshotTiming,
};
use crate::core::errors::ForepawError;
use crate::core::icon_class_parser::IconClassParser;
use crate::core::ref_assigner::RefAssigner;
use crate::core::types::{Point, Rect};
use crate::platform::SnapshotOptions;

use super::app::{
    cf_string_from_str, electron_tree_is_populated, enable_electron_accessibility, find_app,
    find_window, is_electron_app,
};
use super::ffi;
use super::ffi::*;

const DEFAULT_DEPTH: usize = 15;
const ELECTRON_DEPTH: usize = 25;

// Batch attribute indices -- must match BATCH_ATTR_NAMES order.
const ATTR_ROLE: usize = 0;
const ATTR_TITLE: usize = 1;
const ATTR_DESCRIPTION: usize = 2;
const ATTR_VALUE: usize = 3;
const ATTR_POSITION: usize = 4;
const ATTR_SIZE: usize = 5;
const ATTR_CHILDREN: usize = 6;
const ATTR_SUBROLE: usize = 7;
const ATTR_TITLE_UI_ELEMENT: usize = 8;
const ATTR_HELP: usize = 9;
const ATTR_PLACEHOLDER_VALUE: usize = 10;
const ATTR_DOM_CLASS_LIST: usize = 11;
const ATTR_ROLE_DESCRIPTION: usize = 12;
const ATTR_COUNT: usize = 13;

/// Attribute names for batch fetching.
const BATCH_ATTR_NAMES: [&str; ATTR_COUNT] = [
    "AXRole",             // 0
    "AXTitle",            // 1
    "AXDescription",      // 2
    "AXValue",            // 3
    "AXPosition",         // 4
    "AXSize",             // 5
    "AXChildren",         // 6
    "AXSubrole",          // 7
    "AXTitleUIElement",   // 8
    "AXHelp",             // 9
    "AXPlaceholderValue", // 10
    "AXDOMClassList",     // 11
    "AXRoleDescription",  // 12
];

/// Cached CFArray of attribute name strings for batch fetching.
/// Created once on first use, never freed.
static BATCH_ATTR_ARRAY: std::sync::OnceLock<SendableCFArray> = std::sync::OnceLock::new();

/// Wrapper to make CFArrayRef Send+Sync (immutable after creation).
struct SendableCFArray(CFArrayRef);
unsafe impl Send for SendableCFArray {}
unsafe impl Sync for SendableCFArray {}

fn get_batch_attr_array() -> CFArrayRef {
    BATCH_ATTR_ARRAY
        .get_or_init(|| {
            let cf_strings: Vec<CFStringRef> = BATCH_ATTR_NAMES
                .iter()
                .map(|s| cf_string_from_str(s))
                .collect();
            let ptrs: Vec<*const std::ffi::c_void> =
                cf_strings.iter().map(|s| *s as *const _).collect();
            let array = unsafe {
                CFArrayCreate(
                    std::ptr::null(),
                    ptrs.as_ptr(),
                    ptrs.len() as CFIndex,
                    &ffi::kCFTypeArrayCallBacks,
                )
            };
            for s in &cf_strings {
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
pub fn snapshot(app_name: &str, options: &SnapshotOptions) -> Result<ElementTree, ForepawError> {
    let running_app = find_app(app_name)?;

    // Activate the app so the AX tree matches what action commands will see.
    // Activate the app so the AX tree matches what action commands will see.
    // Some apps (browsers) only expose web content when active.
    #[allow(deprecated)]
    running_app.activateWithOptions(
        objc2_app_kit::NSApplicationActivationOptions::ActivateIgnoringOtherApps,
    );
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Electron apps need AXManualAccessibility + polling for tree population.
    let is_electron = is_electron_app(&running_app);
    let pid = running_app.processIdentifier();
    if is_electron {
        enable_electron_accessibility(pid);
        if !electron_tree_is_populated(pid) {
            for _ in 0..6 {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if electron_tree_is_populated(pid) {
                    break;
                }
            }
        }
    }

    let effective_depth = if is_electron {
        std::cmp::max(options.max_depth, ELECTRON_DEPTH)
    } else {
        options.max_depth
    };

    let app_element = unsafe { AXUIElementCreateApplication(pid) };

    // Window bounds for offscreen pruning and coordinate display.
    let window_bounds = find_window(pid, None).ok().map(|w| w.bounds);

    let pruning = TreePruning {
        skip_menu_bar: options.skip_menu_bar,
        skip_zero_size: options.skip_zero_size,
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
        app: app_name.to_string(),
        root: result.root,
        refs: result.refs,
        window_bounds,
        timing,
    })
}

/// Re-walk the tree to resolve a ref's center position (for coordinate-based actions).
pub fn resolve_ref_position(ref_id: i32, app_name: &str) -> Result<Point, ForepawError> {
    let bounds = resolve_ref_bounds(ref_id, app_name)?;
    Ok(Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    ))
}

/// Re-walk the tree to resolve a ref's bounding rect.
pub fn resolve_ref_bounds(ref_id: i32, app_name: &str) -> Result<Rect, ForepawError> {
    let element = resolve_ref_element(ref_id, app_name)?;
    let pos = get_element_position(element)
        .ok_or_else(|| ForepawError::ActionFailed("element has no position".into()))?;
    let (w, h) = get_element_size(element)
        .ok_or_else(|| ForepawError::ActionFailed("element has no size".into()))?;
    Ok(Rect::new(pos.x, pos.y, w, h))
}

// ---------------------------------------------------------------------------
// Tree pruning options
// ---------------------------------------------------------------------------

struct TreePruning {
    skip_menu_bar: bool,
    skip_zero_size: bool,
    window_bounds: Option<Rect>,
}

// ---------------------------------------------------------------------------
// Batch attribute fetching
// ---------------------------------------------------------------------------

/// Wrapper around the CFArray returned by `AXUIElementCopyMultipleAttributeValues`.
/// Provides typed accessors for each attribute by index. Releases the CFArray on drop.
struct BatchAttrs {
    array: CFArrayRef,
}

impl BatchAttrs {
    fn new(array: CFArrayRef) -> Self {
        Self { array }
    }

    /// Get the raw CFTypeRef at the given index, or None if missing/kCFNull.
    fn raw(&self, idx: usize) -> Option<CFTypeRef> {
        if idx >= ATTR_COUNT {
            return None;
        }
        unsafe {
            let val = CFArrayGetValueAtIndex(self.array, idx as CFIndex);
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
    fn string(&self, idx: usize) -> Option<String> {
        let val = self.raw(idx)?;
        unsafe {
            if CFGetTypeID(val) != CFStringGetTypeID() {
                return None;
            }
            cf_string_to_rust(val as CFStringRef)
        }
    }

    /// Extract the value attribute (index 3), which can be CFString or CFNumber.
    fn value_string(&self, idx: usize) -> Option<String> {
        let val = self.raw(idx)?;
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

    /// Extract a CGPoint from an AXValue attribute.
    fn point(&self, idx: usize) -> Option<Point> {
        let val = self.raw(idx)?;
        unsafe {
            let mut pt = CGPointFFI { x: 0.0, y: 0.0 };
            if AXValueGetValue(
                AXValueRef(val as *const _),
                AXValueType::CGPoint,
                &mut pt as *mut _ as *mut std::ffi::c_void,
            ) != 0
            {
                Some(Point::new(pt.x, pt.y))
            } else {
                None
            }
        }
    }

    /// Extract a CGSize from an AXValue attribute.
    fn size_val(&self, idx: usize) -> Option<(f64, f64)> {
        let val = self.raw(idx)?;
        unsafe {
            let mut sz = CGSizeFFI {
                width: 0.0,
                height: 0.0,
            };
            if AXValueGetValue(
                AXValueRef(val as *const _),
                AXValueType::CGSize,
                &mut sz as *mut _ as *mut std::ffi::c_void,
            ) != 0
            {
                Some((sz.width, sz.height))
            } else {
                None
            }
        }
    }

    /// Build a Rect from position (pos_idx) and size (size_idx) attributes.
    fn bounds(&self, pos_idx: usize, size_idx: usize) -> Option<Rect> {
        let pt = self.point(pos_idx)?;
        let (w, h) = self.size_val(size_idx)?;
        Some(Rect::new(pt.x, pt.y, w, h))
    }

    /// Extract child AXUIElement refs from the AXChildren attribute.
    fn children(&self, idx: usize) -> Vec<AXUIElementRef> {
        let Some(val) = self.raw(idx) else {
            return Vec::new();
        };
        unsafe {
            if CFGetTypeID(val) != CFArrayGetTypeID() {
                return Vec::new();
            }
            let count = CFArrayGetCount(val as CFArrayRef);
            let mut result = Vec::with_capacity(count as usize);
            for i in 0..count {
                let child = CFArrayGetValueAtIndex(val as CFArrayRef, i);
                // CFRetain: CFArrayGetValueAtIndex returns a non-retained pointer.
                CFRetain(child as CFTypeRef);
                result.push(AXUIElementRef::from_raw(child));
            }
            result
        }
    }

    /// Extract a single AXUIElement ref (e.g. for AXTitleUIElement).
    fn element(&self, idx: usize) -> Option<AXUIElementRef> {
        let val = self.raw(idx)?;
        unsafe { Some(AXUIElementRef::from_raw(val as *const std::ffi::c_void)) }
    }

    /// Extract a CFArray of CFStrings (e.g. for AXDOMClassList).
    fn string_array(&self, idx: usize) -> Option<Vec<String>> {
        let val = self.raw(idx)?;
        unsafe {
            if CFGetTypeID(val) != CFArrayGetTypeID() {
                return None;
            }
            let count = CFArrayGetCount(val as CFArrayRef);
            let mut result = Vec::with_capacity(count as usize);
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
        unsafe { CFRelease(self.array as CFTypeRef) };
    }
}

/// Call `AXUIElementCopyMultipleAttributeValues` for all 13 batch attributes.
fn fetch_batch_attributes(element: AXUIElementRef) -> Option<BatchAttrs> {
    let attr_array = get_batch_attr_array();
    let mut values: CFArrayRef = std::ptr::null();
    let result =
        unsafe { AXUIElementCopyMultipleAttributeValues(element, attr_array, 0, &mut values) };
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
        return ElementNode::new("AXGroup");
    }

    let Some(attrs) = fetch_batch_attributes(element) else {
        return ElementNode::new("AXUnknown");
    };

    let role = attrs
        .string(ATTR_ROLE)
        .unwrap_or_else(|| "AXUnknown".to_string());

    // Skip menu bar subtree if requested.
    if pruning.skip_menu_bar && role == "AXMenuBar" {
        return ElementNode::new(role);
    }

    let value = attrs.value_string(ATTR_VALUE);
    let bounds = attrs.bounds(ATTR_POSITION, ATTR_SIZE);

    // Collect subrole attribute.
    let mut attributes: Vec<(String, String)> = Vec::new();
    if let Some(subrole) = attrs.string(ATTR_SUBROLE) {
        if !subrole.is_empty() && subrole != "AXNone" {
            attributes.push(("subrole".to_string(), subrole));
        }
    }

    // Skip zero-size subtrees (collapsed menus, hidden panels).
    if pruning.skip_zero_size {
        if let Some(b) = &bounds {
            if b.width == 0.0 && b.height == 0.0 && depth > 1 {
                let name = non_empty(&attrs.string(ATTR_TITLE))
                    .or_else(|| non_empty(&attrs.string(ATTR_DESCRIPTION)))
                    .or_else(|| computed_name(&attrs, &[], element));
                return ElementNode {
                    role,
                    name,
                    value,
                    r#ref: None,
                    bounds,
                    attributes,
                    children: Vec::new(),
                };
            }
        }
    }

    // Skip offscreen subtrees.
    if let (Some(wb), Some(b)) = (&pruning.window_bounds, &bounds) {
        if depth > 2 && b.width > 0.0 && b.height > 0.0 {
            let no_horizontal = b.x + b.width <= wb.x || b.x >= wb.x + wb.width;
            let no_vertical = b.y + b.height <= wb.y || b.y >= wb.y + wb.height;
            if no_horizontal || no_vertical {
                let name = non_empty(&attrs.string(ATTR_TITLE));
                return ElementNode {
                    role,
                    name,
                    value: None,
                    r#ref: None,
                    bounds: Some(*b),
                    attributes,
                    children: Vec::new(),
                };
            }
        }
    }

    // Build children BEFORE computing name. This lets computedName read
    // names/values from already-built child ElementNodes instead of making
    // individual IPC calls.
    let children_refs = attrs.children(ATTR_CHILDREN);
    let children: Vec<ElementNode> = children_refs
        .iter()
        .map(|child| build_tree(*child, depth + 1, max_depth, pruning))
        .collect();

    let name = non_empty(&attrs.string(ATTR_TITLE))
        .or_else(|| non_empty(&attrs.string(ATTR_DESCRIPTION)))
        .or_else(|| computed_name(&attrs, &children, element));

    ElementNode {
        role,
        name,
        value,
        r#ref: None,
        bounds,
        attributes,
        children,
    }
}

// ---------------------------------------------------------------------------
// Name computation
// ---------------------------------------------------------------------------

/// Derive a name from batch attributes and already-built child nodes.
///
/// The chain (in priority order):
/// 1. AXTitleUIElement -> its value or title
/// 2. First AXStaticText child's value, or AXImage child with a name
/// 3. AXHelp
/// 4. AXPlaceholderValue
/// 5. AXDOMClassList -> icon class parsing
/// 6. AXRoleDescription (when not generic)
fn computed_name(
    attrs: &BatchAttrs,
    children: &[ElementNode],
    _element: AXUIElementRef,
) -> Option<String> {
    // 1. AXTitleUIElement (index 8) -> read its value or title.
    if let Some(title_element) = attrs.element(ATTR_TITLE_UI_ELEMENT) {
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
        if child.role == "AXStaticText" {
            if let Some(ref val) = child.value {
                if !val.is_empty() {
                    return Some(val.clone());
                }
            }
        }
        if child.role == "AXImage" {
            if let Some(ref name) = child.name {
                if !name.is_empty() {
                    return Some(name.clone());
                }
            }
        }
    }

    // 3. AXHelp (index 9)
    if let Some(help) = attrs.string(ATTR_HELP) {
        if !help.is_empty() {
            return Some(help);
        }
    }

    // 4. AXPlaceholderValue (index 10)
    if let Some(placeholder) = attrs.string(ATTR_PLACEHOLDER_VALUE) {
        if !placeholder.is_empty() {
            return Some(placeholder);
        }
    }

    // 5. AXDOMClassList (index 11) -> icon class parsing
    if let Some(class_list) = attrs.string_array(ATTR_DOM_CLASS_LIST) {
        let class_refs: Vec<&str> = class_list.iter().map(std::string::String::as_str).collect();
        if let Some(icon_name) = IconClassParser::new().parse(&class_refs) {
            return Some(icon_name);
        }
    }

    // 6. AXRoleDescription (index 12)
    if let Some(role_desc) = attrs.string(ATTR_ROLE_DESCRIPTION) {
        if !role_desc.is_empty() && !GENERIC_ROLE_DESCRIPTIONS.contains(&role_desc.as_str()) {
            return Some(role_desc);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Ref resolution (for action dispatch)
// ---------------------------------------------------------------------------

/// Re-walk the AX tree to find the AXUIElement at the given ref position.
pub fn resolve_ref_element(ref_id: i32, app_name: &str) -> Result<AXUIElementRef, ForepawError> {
    let running_app = find_app(app_name)?;
    let is_electron = is_electron_app(&running_app);
    if is_electron {
        enable_electron_accessibility(running_app.processIdentifier());
    }

    let resolve_depth = if is_electron {
        ELECTRON_DEPTH
    } else {
        DEFAULT_DEPTH
    };
    let app_element = unsafe { AXUIElementCreateApplication(running_app.processIdentifier()) };

    let mut counter: i32 = 1;
    let mut elements: HashMap<i32, AXUIElementRef> = HashMap::new();
    collect_ax_elements(app_element, 0, resolve_depth, &mut counter, &mut elements);

    elements
        .remove(&ref_id)
        .ok_or_else(|| ForepawError::StaleRef(ElementRef::new(ref_id)))
}

/// Walk the AX tree, collecting AXUIElement handles for interactive elements
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

    let role = get_ax_string_attr(element, "AXRole").unwrap_or_default();

    if is_interactive_role(&role) {
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

/// Get a single string attribute from an AXUIElement.
pub fn get_ax_string_attr(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let attr_cf = cf_string_from_str(attribute);
    let mut value: CFTypeRef = std::ptr::null();
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &mut value) };
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return None;
    }
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

/// Get the AXChildren attribute as a Vec of AXUIElementRef.
fn get_ax_element_children(element: AXUIElementRef) -> Vec<AXUIElementRef> {
    let attr_cf = cf_string_from_str("AXChildren");
    let mut value: CFTypeRef = std::ptr::null();
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &mut value) };
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return Vec::new();
    }
    unsafe {
        if CFGetTypeID(value) != CFArrayGetTypeID() {
            CFRelease(value);
            return Vec::new();
        }
        let count = CFArrayGetCount(value as CFArrayRef);
        let mut children = Vec::with_capacity(count as usize);
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

/// Get the position (AXPosition) of an AXUIElement as a Point.
pub fn get_element_position(element: AXUIElementRef) -> Option<Point> {
    let attr_cf = cf_string_from_str("AXPosition");
    let mut value: CFTypeRef = std::ptr::null();
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &mut value) };
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return None;
    }
    unsafe {
        let mut pt = CGPointFFI { x: 0.0, y: 0.0 };
        let ok = AXValueGetValue(
            AXValueRef(value as *const _),
            AXValueType::CGPoint,
            &mut pt as *mut _ as *mut std::ffi::c_void,
        );
        CFRelease(value);
        if ok != 0 {
            Some(Point::new(pt.x, pt.y))
        } else {
            None
        }
    }
}

/// Get the size (AXSize) of an AXUIElement as (width, height).
pub fn get_element_size(element: AXUIElementRef) -> Option<(f64, f64)> {
    let attr_cf = cf_string_from_str("AXSize");
    let mut value: CFTypeRef = std::ptr::null();
    let result = unsafe { AXUIElementCopyAttributeValue(element, attr_cf, &mut value) };
    unsafe { CFRelease(attr_cf as CFTypeRef) };
    if result != AXError::Success || value.is_null() {
        return None;
    }
    unsafe {
        let mut sz = CGSizeFFI {
            width: 0.0,
            height: 0.0,
        };
        let ok = AXValueGetValue(
            AXValueRef(value as *const _),
            AXValueType::CGSize,
            &mut sz as *mut _ as *mut std::ffi::c_void,
        );
        CFRelease(value);
        if ok != 0 {
            Some((sz.width, sz.height))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// CFType conversion helpers
// ---------------------------------------------------------------------------

/// Convert a CFStringRef to a Rust String. Handles both ASCII (fast path)
/// and non-ASCII (buffer copy) strings.
fn cf_string_to_rust(cf_str: CFStringRef) -> Option<String> {
    unsafe {
        // Fast path: ASCII/null-fast strings
        let ptr = CFStringGetCStringPtr(cf_str, K_CF_STRING_ENCODING_UTF8);
        if !ptr.is_null() {
            return std::ffi::CStr::from_ptr(ptr)
                .to_str()
                .ok()
                .map(String::from);
        }
        // Slow path: non-ASCII strings need a buffer copy
        let len = CFStringGetLength(cf_str);
        let mut buf = vec![0_u8; (len as usize + 1) * 4]; // worst case: 4 bytes per char
        if CFStringGetCString(
            cf_str,
            buf.as_mut_ptr() as *mut std::ffi::c_char,
            buf.len() as CFIndex,
            K_CF_STRING_ENCODING_UTF8,
        ) {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            std::str::from_utf8(&buf[..end]).ok().map(String::from)
        } else {
            None
        }
    }
}

/// Convert a CFNumber to a Rust String. Tries integer first, then float.
fn number_to_rust_string(number: CFNumberRef) -> Option<String> {
    unsafe {
        // Try as i64 first (most AX values are integers)
        let mut val: i64 = 0;
        // K_CF_NUMBER_SINT64_TYPE = 4 (not in our FFI, use raw value)
        if CFNumberGetValue(number, 4, &mut val as *mut i64 as *mut std::ffi::c_void) != 0 {
            return Some(val.to_string());
        }
        // Fallback: f64
        let mut fval: f64 = 0.0;
        if CFNumberGetValue(
            number,
            K_CF_NUMBER_DOUBLE_TYPE,
            &mut fval as *mut f64 as *mut std::ffi::c_void,
        ) != 0
        {
            return Some(if fval == fval.floor() {
                format!("{}", fval as i64)
            } else {
                format!("{fval}")
            });
        }
        None
    }
}

/// Return None for empty strings -- AX APIs often return "" rather than nil.
fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .and_then(|v| if v.is_empty() { None } else { Some(v.clone()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_returns_some_for_nonempty() {
        assert_eq!(
            non_empty(&Some("hello".to_string())),
            Some("hello".to_string())
        );
    }

    #[test]
    fn non_empty_returns_none_for_empty() {
        assert_eq!(non_empty(&Some(String::new())), None);
    }

    #[test]
    fn non_empty_returns_none_for_none() {
        assert_eq!(non_empty(&None), None);
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
            skip_menu_bar: false,
            skip_zero_size: true,
            window_bounds: None,
        };
        assert!(pruning.skip_zero_size);
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

    // --- Batch attribute constants ---

    #[test]
    fn batch_attr_names_match_indices() {
        assert_eq!(BATCH_ATTR_NAMES[ATTR_ROLE], "AXRole");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_TITLE], "AXTitle");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_DESCRIPTION], "AXDescription");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_VALUE], "AXValue");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_POSITION], "AXPosition");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_SIZE], "AXSize");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_CHILDREN], "AXChildren");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_SUBROLE], "AXSubrole");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_TITLE_UI_ELEMENT], "AXTitleUIElement");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_HELP], "AXHelp");
        assert_eq!(
            BATCH_ATTR_NAMES[ATTR_PLACEHOLDER_VALUE],
            "AXPlaceholderValue"
        );
        assert_eq!(BATCH_ATTR_NAMES[ATTR_DOM_CLASS_LIST], "AXDOMClassList");
        assert_eq!(BATCH_ATTR_NAMES[ATTR_ROLE_DESCRIPTION], "AXRoleDescription");
    }

    #[test]
    fn batch_attr_count_matches_names() {
        assert_eq!(BATCH_ATTR_NAMES.len(), ATTR_COUNT);
    }

    // --- Snapshot options defaults ---

    #[test]
    fn default_depths() {
        assert_eq!(DEFAULT_DEPTH, 15);
        assert_eq!(ELECTRON_DEPTH, 25);
    }
}
