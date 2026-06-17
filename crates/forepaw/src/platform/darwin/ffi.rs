//! Raw FFI bindings to macOS Accessibility, CoreGraphics, and CoreFoundation APIs.
//!
//! These are used directly instead of the `accessibility` crate because:
//! 1. The crate's `AXUIElement` is !Send + !Sync (conflicts with `DesktopProvider`)
//! 2. We need `AXUIElementCopyMultipleAttributeValues` (not wrapped by the crate)
//! 3. We need arbitrary attribute setting (`AXManualAccessibility`, `AXFocused`)
//!
//! All functions are linked against the `ApplicationServices` framework.

use std::ffi::c_void;

use objc2_core_foundation::CGFloat;

// ---------------------------------------------------------------------------
// CoreFoundation types (thin wrappers around pointers)
// ---------------------------------------------------------------------------

pub(super) type CFIndex = i64;
pub(super) type CFTypeID = u64;
pub(super) type CFHashCode = u64;
pub(super) type Boolean = u8;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFAllocator(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFString(c_void);

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFArray(c_void);

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFDictionary(c_void);

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFNumber(c_void);

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFBoolean(c_void);

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFNull(c_void);

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFData(c_void);

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFType(c_void);

pub(super) type CFTypeRef = *const CFType;
pub(super) type CFStringRef = *const CFString;
pub(super) type CFArrayRef = *const CFArray;
pub(super) type CFDictionaryRef = *const CFDictionary;
pub(super) type CFNumberRef = *const CFNumber;
pub(super) type CFBooleanRef = *const CFBoolean;
pub(super) type CFDataRef = *const CFData;
pub(super) type CFAllocatorRef = *const CFAllocator;

// CFArray callbacks -- use the system-provided global kCFTypeArrayCallBacks
// (which has actual retain/release/equal functions, not a zeroed struct).
#[repr(C)]
pub(super) struct CFArrayCallBacks {
    pub version: CFIndex,
    pub r#retain: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void) -> *const c_void>,
    pub release: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void)>,
    pub copy_description: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
    pub equal: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> Boolean>,
}

// CFDictionary callbacks -- we use the system-provided globals
// (kCFTypeDictionaryKeyCallBacks / kCFTypeDictionaryValueCallBacks)
// so we only need the struct definitions for the FFI extern block.
#[repr(C)]
pub(super) struct CFDictionaryKeyCallBacks {
    pub version: CFIndex,
    pub r#retain: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void) -> *const c_void>,
    pub release: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void)>,
    pub copy_description: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
    pub equal: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> Boolean>,
    pub hash: Option<unsafe extern "C" fn(*const c_void) -> u64>,
}

#[repr(C)]
pub(super) struct CFDictionaryValueCallBacks {
    pub version: CFIndex,
    pub r#retain: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void) -> *const c_void>,
    pub release: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void)>,
    pub copy_description: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
    pub equal: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> Boolean>,
}

// ---------------------------------------------------------------------------
// Accessibility (AXUIElement) - linked via ApplicationServices
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AXUIElementRef(pub *const c_void);

// SAFETY: AXUIElementRef is a CF object pointer. It's safe to send between
// threads as long as we don't share mutable references.
// SAFETY: AXUIElementRef is a thread-safe opaque reference to an accessibility
// object. Apple's AX APIs are safe to call from any thread.
unsafe impl Send for AXUIElementRef {}
// SAFETY: Same reasoning as Send above.
unsafe impl Sync for AXUIElementRef {}

impl AXUIElementRef {
    /// Create an `AXUIElementRef` from a raw pointer (e.g. from `CFArrayGetValueAtIndex`).
    pub unsafe fn from_raw(ptr: *const c_void) -> Self {
        Self(ptr)
    }
}

#[repr(isize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum AXError {
    Success = 0,
    Failure = -25200,
    IllegalArgument = -25201,
    InvalidUIElement = -25202,
    InvalidUIElementObserver = -25203,
    CannotComplete = -25204,
    AttributeUnsupported = -25205,
    ActionUnsupported = -25206,
    NotificationUnsupported = -25207,
    NotImplemented = -25208,
    NotificationAlreadyRegistered = -25209,
    NotificationNotRegistered = -25210,
    APIDisabled = -25211,
    NoValue = -25212,
    ParameterizedAttributeUnsupported = -25213,
    NotEnoughPrecision = -25214,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub(super) struct AXValueRef(pub *const c_void);

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[expect(clippy::enum_variant_names, reason = "Apple API naming")]
pub(super) enum AXValueType {
    CGPoint = 1,
    CGSize = 2,
    CGRect = 3,
    CFRange = 4,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub(super) fn AXIsProcessTrusted() -> Boolean;
    pub(super) fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    pub(super) fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub(super) fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    pub(super) fn AXUIElementCopyMultipleAttributeValues(
        element: AXUIElementRef,
        attributes: CFArrayRef,
        options: u32,
        values: *mut CFArrayRef,
    ) -> AXError;
    pub(super) fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    pub(super) fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef)
        -> AXError;
    pub(super) fn AXValueGetType(value: AXValueRef) -> AXValueType;
    pub(super) fn AXValueGetValue(
        value: AXValueRef,
        the_type: AXValueType,
        value_ptr: *mut c_void,
    ) -> Boolean;
    pub(super) fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;

    // Hit testing and system-wide element
    pub(super) fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub(super) fn AXUIElementCopyElementAtPosition(
        element: AXUIElementRef,
        x: f32,
        y: f32,
        result: *mut AXUIElementRef,
    ) -> AXError;

    // Action names (for HitTestResult.actions)
    pub(super) fn AXUIElementCopyActionNames(
        element: AXUIElementRef,
        action_names: *mut CFArrayRef,
    ) -> AXError;

    // Parameterized attributes (for text-level attributes)
    pub(super) fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        parameterized_attribute: CFStringRef,
        parameter: CFTypeRef,
        result: *mut CFTypeRef,
    ) -> AXError;

    pub(super) fn AXValueCreate(the_type: AXValueType, value_ptr: *const c_void) -> AXValueRef;

    pub(super) fn AXUIElementCopyAttributeNames(
        element: AXUIElementRef,
        names: *mut CFArrayRef,
    ) -> AXError;

    pub(super) fn AXUIElementCopyParameterizedAttributeNames(
        element: AXUIElementRef,
        names: *mut CFArrayRef,
    ) -> AXError;
}

// ---------------------------------------------------------------------------
// CoreGraphics - Window listing
// ---------------------------------------------------------------------------

pub(super) type CGWindowID = u32;

pub(super) type CGWindowListOption = u32;
pub(super) const CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: CGWindowListOption = 1 << 0;

pub(super) type CGWindowImageOption = u32;
pub(super) const CG_WINDOW_IMAGE_BEST_RESOLUTION: CGWindowImageOption = 1 << 0;
pub(super) const CG_WINDOW_IMAGE_NOMINAL_RESOLUTION: CGWindowImageOption = 1 << 1;
pub(super) const CG_WINDOW_IMAGE_BOUNDS_IGNORE_FRAMING: CGWindowImageOption = 1 << 2;

pub(super) const K_CG_NULL_WINDOW_ID: CGWindowID = 0;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub(super) fn CGWindowListCopyWindowInfo(
        options: CGWindowListOption,
        relativeToWindow: CGWindowID,
    ) -> CFArrayRef;
    pub(super) fn CGPreflightScreenCaptureAccess() -> Boolean;
    pub(super) fn CGRequestScreenCaptureAccess() -> Boolean;
    pub(super) fn CGMainDisplayID() -> u32;
    pub(super) fn CGGetOnlineDisplayList(
        max_displays: u32,
        online_displays: *mut u32,
        display_count: *mut u32,
    ) -> i32;
    pub(super) fn CGDisplayBounds(display: u32) -> CGRectFFI;
}

// ---------------------------------------------------------------------------
// CoreGraphics - Event types
// ---------------------------------------------------------------------------

pub(super) type CGKeyCode = u16;
pub(super) type CGEventType = u32;
pub(super) type CGMouseButton = u32;

pub(super) const K_CG_EVENT_LEFT_MOUSE_DOWN: CGEventType = 1;
pub(super) const K_CG_EVENT_LEFT_MOUSE_UP: CGEventType = 2;
pub(super) const K_CG_EVENT_RIGHT_MOUSE_DOWN: CGEventType = 3;
pub(super) const K_CG_EVENT_RIGHT_MOUSE_UP: CGEventType = 4;
pub(super) const K_CG_EVENT_MOUSE_MOVED: CGEventType = 5;
pub(super) const K_CG_EVENT_LEFT_MOUSE_DRAGGED: CGEventType = 6;
pub(super) const K_CG_EVENT_RIGHT_MOUSE_DRAGGED: CGEventType = 7;
pub(super) const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
pub(super) const K_CG_EVENT_KEY_UP: CGEventType = 11;
pub(super) const K_CG_EVENT_SCROLL_WHEEL: CGEventType = 22;

pub(super) const K_CG_MOUSE_BUTTON_LEFT: CGMouseButton = 0;
pub(super) const K_CG_MOUSE_BUTTON_RIGHT: CGMouseButton = 1;

pub(super) const K_CG_EVENT_TAP_DEFAULT: u32 = 0;
pub(super) const K_CG_EVENT_TAP_CGHID: u32 = 1;

pub(super) const K_CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;
pub(super) const K_CG_SCROLL_EVENT_UNIT_PIXEL: u32 = 0;

// CGEventFields
pub(super) const K_CG_MOUSE_EVENT_CLICK_STATE: u32 = 1;
pub(super) const K_CG_MOUSE_EVENT_PRESSURE: u32 = 2;

// CGEventFlags
pub(super) type CGEventFlags = u64;
pub(super) const K_CG_EVENT_FLAG_CMD: CGEventFlags = 0x0010_0000;
pub(super) const K_CG_EVENT_FLAG_SHIFT: CGEventFlags = 0x00200;
pub(super) const K_CG_EVENT_FLAG_ALT: CGEventFlags = 0x0008_0000;
pub(super) const K_CG_EVENT_FLAG_CTRL: CGEventFlags = 0x0004_0000;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGEvent(c_void);

pub(super) type CGEventRef = *mut CGEvent;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGEventSource(c_void);

pub(super) type CGEventSourceRef = *mut CGEventSource;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    // Event creation
    pub(super) fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouse_type: CGEventType,
        mouse_cursor_position: CGPointFFI,
        mouse_button: CGMouseButton,
    ) -> CGEventRef;
    pub(super) fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: CGKeyCode,
        key_down: Boolean,
    ) -> CGEventRef;
    pub(super) fn CGEventCreateScrollWheelEvent(
        source: CGEventSourceRef,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;

    // Event posting and manipulation
    pub(super) fn CGEventPost(tap: u32, event: CGEventRef);
    pub(super) fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
    pub(super) fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
    pub(super) fn CGEventSetType(event: CGEventRef, event_type: CGEventType);
    pub(super) fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub(super) fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    pub(super) fn CGEventSetDoubleValueField(event: CGEventRef, field: u32, value: CGFloat);
    pub(super) fn CGEventKeyboardSetUnicodeString(
        event: CGEventRef,
        length: u32,
        unicode_string: *const u16,
    );
    pub(super) fn CGEventGetLocation(event: CGEventRef) -> CGPointFFI;

    // Memory management
    pub(super) fn CFRelease(cf: CFTypeRef);
    pub(super) fn CFRetain(cf: CFTypeRef);

    // CGImage (for screenshot/annotation)
    pub(super) fn CGWindowListCreateImage(
        screen_bounds: CGRectFFI,
        list_option: CGWindowListOption,
        window_id: CGWindowID,
        image_option: CGWindowImageOption,
    ) -> CGImageRef;
}

// ---------------------------------------------------------------------------
// CoreGraphics - Image types (for annotation rendering)
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGImage(c_void);

pub(super) type CGImageRef = *mut CGImage;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGColorSpace(c_void);

pub(super) type CGColorSpaceRef = *mut CGColorSpace;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGContext(c_void);

pub(super) type CGContextRef = *mut CGContext;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGDataProvider(c_void);

pub(super) type CGDataProviderRef = *mut CGDataProvider;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGColor(c_void);

pub(super) type CGColorRef = *mut CGColor;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGPath(c_void);

pub(super) type CGPathRef = *const CGPath;
pub(super) type CGMutablePathRef = *mut CGPath;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CGImageDestination(c_void);

pub(super) type CGImageDestinationRef = *mut CGImageDestination;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CGPointFFI {
    pub x: CGFloat,
    pub y: CGFloat,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub(super) struct CGSizeFFI {
    pub width: CGFloat,
    pub height: CGFloat,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub(super) struct CGRectFFI {
    pub origin: CGPointFFI,
    pub size: CGSizeFFI,
}

// CoreGraphics image API
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub(super) fn CGColorSpaceCreateDeviceRGB() -> CGColorSpaceRef;
    pub(super) fn CGDataProviderCreateWithFilename(filename: *const i8) -> CGDataProviderRef;
    pub(super) fn CGImageCreateWithPNGDataProvider(
        source: CGDataProviderRef,
        decode: *const CGFloat,
        should_interpolate: Boolean,
        intent: u32,
    ) -> CGImageRef;
    pub(super) fn CGImageGetWidth(image: CGImageRef) -> usize;
    pub(super) fn CGImageGetHeight(image: CGImageRef) -> usize;
    pub(super) fn CGImageCreateWithImageInRect(image: CGImageRef, rect: CGRectFFI) -> CGImageRef;
    pub(super) fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: CGColorSpaceRef,
        bitmap_info: u32,
    ) -> CGContextRef;
    pub(super) fn CGContextDrawImage(ctx: CGContextRef, rect: CGRectFFI, image: CGImageRef);
    pub(super) fn CGContextFillPath(ctx: CGContextRef);
    pub(super) fn CGContextStrokePath(ctx: CGContextRef);
    pub(super) fn CGContextAddPath(ctx: CGContextRef, path: CGPathRef);
    pub(super) fn CGContextSetFillColorWithColor(ctx: CGContextRef, color: CGColorRef);
    pub(super) fn CGContextSetStrokeColorWithColor(ctx: CGContextRef, color: CGColorRef);
    pub(super) fn CGContextSetLineWidth(ctx: CGContextRef, width: CGFloat);
    pub(super) fn CGContextStrokeRect(ctx: CGContextRef, rect: CGRectFFI);
    pub(super) fn CGContextFillRect(ctx: CGContextRef, rect: CGRectFFI);
    pub(super) fn CGBitmapContextCreateImage(ctx: CGContextRef) -> CGImageRef;
    pub(super) fn CGPathCreateWithRoundedRect(
        rect: CGRectFFI,
        corner_width: CGFloat,
        corner_height: CGFloat,
        transform: *const c_void,
    ) -> CGPathRef;
    pub(super) fn CGPathCreateMutable() -> CGMutablePathRef;
    pub(super) fn CGPathAddRect(path: CGMutablePathRef, transform: *const c_void, rect: CGRectFFI);
    pub(super) fn CGPathAddRoundedRect(
        path: CGMutablePathRef,
        transform: *const c_void,
        rect: CGRectFFI,
        corner_width: CGFloat,
        corner_height: CGFloat,
    );
    pub(super) fn CGColorCreate(
        rgb_color_space: CGColorSpaceRef,
        components: *const CGFloat,
    ) -> CGColorRef;
    pub(super) fn CGColorGetTypeID() -> CFTypeID;
    pub(super) fn CGColorGetNumberOfComponents(color: CGColorRef) -> usize;
    pub(super) fn CGColorGetComponents(color: CGColorRef) -> *const CGFloat;
    pub(super) fn CGImageDestinationCreateWithURL(
        url: *const c_void,
        format: *const c_void,
        count: usize,
        options: *const c_void,
    ) -> CGImageDestinationRef;
    pub(super) fn CGImageDestinationAddImage(
        dest: CGImageDestinationRef,
        image: CGImageRef,
        properties: *const c_void,
    );
    pub(super) fn CGImageDestinationFinalize(dest: CGImageDestinationRef) -> Boolean;

    // Context state and drawing helpers
    pub(super) fn CGContextSaveGState(ctx: CGContextRef);
    pub(super) fn CGContextRestoreGState(ctx: CGContextRef);
    pub(super) fn CGContextSetTextPosition(ctx: CGContextRef, x: CGFloat, y: CGFloat);
    pub(super) fn CGContextEOFillPath(ctx: CGContextRef);
    pub(super) fn CGBitmapContextGetHeight(ctx: CGContextRef) -> usize;
    pub(super) fn CGBitmapContextGetWidth(ctx: CGContextRef) -> usize;
    pub(super) fn CGContextMoveToPoint(ctx: CGContextRef, x: CGFloat, y: CGFloat);
    pub(super) fn CGContextAddLineToPoint(ctx: CGContextRef, x: CGFloat, y: CGFloat);

    // Null rect for full-screen capture
    pub(super) static kCGNullRect: CGRectFFI;
}

// ---------------------------------------------------------------------------
// CoreFoundation helper functions
// ---------------------------------------------------------------------------

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub(super) fn CFArrayCreate(
        allocator: *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        callbacks: *const CFArrayCallBacks,
    ) -> CFArrayRef;
    pub(super) fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    /// Returns a non-retained pointer to the value at the given index.
    /// Callers must `CFRetain` the returned pointer if they need to keep it
    /// beyond the lifetime of the array.
    pub(super) fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: CFIndex) -> *const c_void;
    pub(super) fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
    pub(super) fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        key_callbacks: *const CFDictionaryKeyCallBacks,
        value_callbacks: *const CFDictionaryValueCallBacks,
    ) -> CFDictionaryRef;
    pub(super) fn CFStringGetCStringPtr(string: CFStringRef, encoding: u32) -> *const i8;
    pub(super) fn CFStringGetCString(
        string: CFStringRef,
        buffer: *mut std::ffi::c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> bool;
    pub(super) fn CFStringGetLength(string: CFStringRef) -> CFIndex;
    pub(super) fn CFNumberGetValue(
        number: CFNumberRef,
        the_type: u32,
        value_ptr: *mut c_void,
    ) -> Boolean;
    pub(super) fn CFBooleanGetValue(boolean: CFBooleanRef) -> Boolean;
    pub(super) fn CFDataGetLength(data: CFDataRef) -> CFIndex;
    pub(super) fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
    pub(super) fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    pub(super) fn CFStringGetTypeID() -> CFTypeID;
    pub(super) fn CFNumberGetTypeID() -> CFTypeID;
    pub(super) fn CFBooleanGetTypeID() -> CFTypeID;
    pub(super) fn CFArrayGetTypeID() -> CFTypeID;
    pub(super) fn CFDictionaryGetTypeID() -> CFTypeID;
    pub(super) fn CFDataGetTypeID() -> CFTypeID;

    // CGWindowListCopyWindowInfo keys (defined as CFStringRef globals)
    pub(super) static kCGWindowNumber: CFStringRef;
    pub(super) static kCGWindowOwnerName: CFStringRef;
    pub(super) static kCGWindowOwnerPID: CFStringRef;
    pub(super) static kCGWindowName: CFStringRef;
    pub(super) static kCGWindowBounds: CFStringRef;
    pub(super) static kCGWindowLayer: CFStringRef;

    // CFDictionary callback globals
    pub(super) static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
    pub(super) static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;

    // CFArray callback global
    pub(super) static kCFTypeArrayCallBacks: CFArrayCallBacks;

    // CFBoolean constants
    pub(super) static kCFBooleanTrue: CFTypeRef;
}

// CFNumber type constants
pub(super) const K_CF_NUMBER_SINT32_TYPE: u32 = 3;
pub(super) const K_CF_NUMBER_DOUBLE_TYPE: u32 = 6;
pub(super) const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

// Null singleton
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub(super) static kCFNull: CFTypeRef;
}

// ---------------------------------------------------------------------------
// CoreText - Font + Line rendering (for annotation text)
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug)]
pub(super) struct CTFont(c_void);

pub(super) type CTFontRef = *const CTFont;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CTLine(c_void);

pub(super) type CTLineRef = *const CTLine;

#[repr(C)]
#[derive(Debug)]
pub(super) struct CFAttributedString(c_void);

pub(super) type CFAttributedStringRef = *const CFAttributedString;

/// CoreFoundation range struct.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(super) struct CFRange {
    pub location: CFIndex,
    pub length: CFIndex,
}
pub(super) type CFRangeType = CFRange;

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    pub(super) fn CTFontCreateWithName(
        name: CFStringRef,
        size: CGFloat,
        matrix: *const c_void,
    ) -> CTFontRef;
    pub(super) fn CTLineCreateWithAttributedString(string: CFAttributedStringRef) -> CTLineRef;
    pub(super) fn CTLineGetBoundsWithOptions(line: CTLineRef, options: u32) -> CGRectFFI;
    pub(super) fn CTLineDraw(line: CTLineRef, context: CGContextRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub(super) fn CFAttributedStringCreate(
        allocator: *const c_void,
        string: CFStringRef,
        attributes: CFDictionaryRef,
    ) -> CFAttributedStringRef;

    pub(super) fn CFAttributedStringGetString(a_str: CFAttributedStringRef) -> CFStringRef;

    pub(super) fn CFAttributedStringGetAttributes(
        a_str: CFAttributedStringRef,
        loc: CFIndex,
        effective_range: *mut CFRangeType,
    ) -> CFDictionaryRef;

    pub(super) fn CFAttributedStringGetTypeID() -> CFTypeID;
}

// CoreText attribute key constants (defined as CFStringRef globals)
#[link(name = "CoreText", kind = "framework")]
extern "C" {
    pub(super) static kCTFontAttributeName: CFStringRef;
    pub(super) static kCTForegroundColorAttributeName: CFStringRef;
}
