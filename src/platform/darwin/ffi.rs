//! Raw FFI bindings to macOS Accessibility, CoreGraphics, and CoreFoundation APIs.
//!
//! These are used directly instead of the `accessibility` crate because:
//! 1. The crate's AXUIElement is !Send + !Sync (conflicts with DesktopProvider)
//! 2. We need AXUIElementCopyMultipleAttributeValues (not wrapped by the crate)
//! 3. We need arbitrary attribute setting (AXManualAccessibility, AXFocused)
//!
//! All functions are linked against the ApplicationServices framework.

use std::ffi::c_void;

use objc2_core_foundation::CGFloat;

// ---------------------------------------------------------------------------
// CoreFoundation types (thin wrappers around pointers)
// ---------------------------------------------------------------------------

pub type CFIndex = i64;
pub type CFTypeID = u64;
pub type CFHashCode = u64;
pub type Boolean = u8;

#[repr(C)]
#[derive(Debug)]
pub struct CFAllocator(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFString(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFArray(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFDictionary(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFNumber(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFBoolean(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFNull(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFData(c_void);

#[repr(C)]
#[derive(Debug)]
pub struct CFType(c_void);

pub type CFTypeRef = *const CFType;
pub type CFStringRef = *const CFString;
pub type CFArrayRef = *const CFArray;
pub type CFDictionaryRef = *const CFDictionary;
pub type CFNumberRef = *const CFNumber;
pub type CFBooleanRef = *const CFBoolean;
pub type CFDataRef = *const CFData;
pub type CFAllocatorRef = *const CFAllocator;

// CFArray callbacks -- use the system-provided global kCFTypeArrayCallBacks
// (which has actual retain/release/equal functions, not a zeroed struct).
#[repr(C)]
pub struct CFArrayCallBacks {
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
pub struct CFDictionaryKeyCallBacks {
    pub version: CFIndex,
    pub r#retain: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void) -> *const c_void>,
    pub release: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void)>,
    pub copy_description: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
    pub equal: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> Boolean>,
    pub hash: Option<unsafe extern "C" fn(*const c_void) -> u64>,
}

#[repr(C)]
pub struct CFDictionaryValueCallBacks {
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
// threads as long as we don't share mutable references. The Swift version
// uses @unchecked Sendable for the same reason.
unsafe impl Send for AXUIElementRef {}
unsafe impl Sync for AXUIElementRef {}

impl AXUIElementRef {
    /// Create an AXUIElementRef from a raw pointer (e.g. from CFArrayGetValueAtIndex).
    pub unsafe fn from_raw(ptr: *const std::ffi::c_void) -> Self {
        Self(ptr)
    }
}

#[repr(isize)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AXError {
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
pub struct AXValueRef(pub *const c_void);

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // FFI bindings match Apple's naming
pub enum AXValueType {
    CGPoint = 1,
    CGSize = 2,
    CGRect = 3,
    CFRange = 4,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXIsProcessTrusted() -> Boolean;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementCopyMultipleAttributeValues(
        element: AXUIElementRef,
        attributes: CFArrayRef,
        options: u32,
        values: *mut CFArrayRef,
    ) -> AXError;
    pub fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    pub fn AXValueGetType(value: AXValueRef) -> AXValueType;
    pub fn AXValueGetValue(value: AXValueRef, the_type: AXValueType, value_ptr: *mut c_void) -> Boolean;
    pub fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
}

// ---------------------------------------------------------------------------
// CoreGraphics - Window listing
// ---------------------------------------------------------------------------

pub type CGWindowID = u32;

pub type CGWindowListOption = u32;
pub const CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: CGWindowListOption = 1 << 0;

pub type CGWindowImageOption = u32;
pub const CG_WINDOW_IMAGE_BEST_RESOLUTION: CGWindowImageOption = 1 << 0;
pub const CG_WINDOW_IMAGE_NOMINAL_RESOLUTION: CGWindowImageOption = 1 << 1;
pub const CG_WINDOW_IMAGE_BOUNDS_IGNORE_FRAMING: CGWindowImageOption = 1 << 2;

pub const K_CG_NULL_WINDOW_ID: CGWindowID = 0;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn CGWindowListCopyWindowInfo(
        options: CGWindowListOption,
        relativeToWindow: CGWindowID,
    ) -> CFArrayRef;
    pub fn CGPreflightScreenCaptureAccess() -> Boolean;
    pub fn CGRequestScreenCaptureAccess() -> Boolean;
    pub fn CGMainDisplayID() -> u32;
}

// ---------------------------------------------------------------------------
// CoreGraphics - Event types
// ---------------------------------------------------------------------------

pub type CGKeyCode = u16;
pub type CGEventType = u32;
pub type CGMouseButton = u32;

pub const K_CG_EVENT_LEFT_MOUSE_DOWN: CGEventType = 1;
pub const K_CG_EVENT_LEFT_MOUSE_UP: CGEventType = 2;
pub const K_CG_EVENT_RIGHT_MOUSE_DOWN: CGEventType = 3;
pub const K_CG_EVENT_RIGHT_MOUSE_UP: CGEventType = 4;
pub const K_CG_EVENT_MOUSE_MOVED: CGEventType = 5;
pub const K_CG_EVENT_LEFT_MOUSE_DRAGGED: CGEventType = 6;
pub const K_CG_EVENT_RIGHT_MOUSE_DRAGGED: CGEventType = 7;
pub const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
pub const K_CG_EVENT_KEY_UP: CGEventType = 11;
pub const K_CG_EVENT_SCROLL_WHEEL: CGEventType = 22;

pub const K_CG_MOUSE_BUTTON_LEFT: CGMouseButton = 0;
pub const K_CG_MOUSE_BUTTON_RIGHT: CGMouseButton = 1;

pub const K_CG_EVENT_TAP_DEFAULT: u32 = 0;
pub const K_CG_EVENT_TAP_CGHID: u32 = 1;

pub const K_CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;
pub const K_CG_SCROLL_EVENT_UNIT_PIXEL: u32 = 0;

// CGEventFields
pub const K_CG_MOUSE_EVENT_CLICK_STATE: u32 = 1;
pub const K_CG_MOUSE_EVENT_PRESSURE: u32 = 2;

// CGEventFlags
pub type CGEventFlags = u64;
pub const K_CG_EVENT_FLAG_CMD: CGEventFlags = 0x00100000;
pub const K_CG_EVENT_FLAG_SHIFT: CGEventFlags = 0x00200;
pub const K_CG_EVENT_FLAG_ALT: CGEventFlags = 0x00080000;
pub const K_CG_EVENT_FLAG_CTRL: CGEventFlags = 0x00040000;

#[repr(C)]
#[derive(Debug)]
pub struct CGEvent(c_void);

pub type CGEventRef = *mut CGEvent;

#[repr(C)]
#[derive(Debug)]
pub struct CGEventSource(c_void);

pub type CGEventSourceRef = *mut CGEventSource;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    // Event creation
    pub fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouse_type: CGEventType,
        mouse_cursor_position: CGPointFFI,
        mouse_button: CGMouseButton,
    ) -> CGEventRef;
    pub fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: CGKeyCode,
        key_down: Boolean,
    ) -> CGEventRef;
    pub fn CGEventCreateScrollWheelEvent(
        source: CGEventSourceRef,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;

    // Event posting and manipulation
    pub fn CGEventPost(tap: u32, event: CGEventRef);
    pub fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
    pub fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
    pub fn CGEventSetType(event: CGEventRef, event_type: CGEventType);
    pub fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    pub fn CGEventSetDoubleValueField(event: CGEventRef, field: u32, value: CGFloat);
    pub fn CGEventKeyboardSetUnicodeString(
        event: CGEventRef,
        length: u32,
        unicode_string: *const u16,
    );
    pub fn CGEventGetLocation(event: CGEventRef) -> CGPointFFI;

    // Memory management
    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFRetain(cf: CFTypeRef);

    // CGImage (for screenshot/annotation)
    pub fn CGWindowListCreateImage(
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
pub struct CGImage(c_void);

pub type CGImageRef = *mut CGImage;

#[repr(C)]
#[derive(Debug)]
pub struct CGColorSpace(c_void);

pub type CGColorSpaceRef = *mut CGColorSpace;

#[repr(C)]
#[derive(Debug)]
pub struct CGContext(c_void);

pub type CGContextRef = *mut CGContext;

#[repr(C)]
#[derive(Debug)]
pub struct CGDataProvider(c_void);

pub type CGDataProviderRef = *mut CGDataProvider;

#[repr(C)]
#[derive(Debug)]
pub struct CGColor(c_void);

pub type CGColorRef = *mut CGColor;

#[repr(C)]
#[derive(Debug)]
pub struct CGPath(c_void);

pub type CGPathRef = *const CGPath;
pub type CGMutablePathRef = *mut CGPath;

#[repr(C)]
#[derive(Debug)]
pub struct CGImageDestination(c_void);

pub type CGImageDestinationRef = *mut CGImageDestination;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CGPointFFI {
    pub x: CGFloat,
    pub y: CGFloat,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CGSizeFFI {
    pub width: CGFloat,
    pub height: CGFloat,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CGRectFFI {
    pub origin: CGPointFFI,
    pub size: CGSizeFFI,
}

// CoreGraphics image API
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn CGColorSpaceCreateDeviceRGB() -> CGColorSpaceRef;
    pub fn CGDataProviderCreateWithFilename(filename: *const i8) -> CGDataProviderRef;
    pub fn CGImageCreateWithPNGDataProvider(
        source: CGDataProviderRef,
        decode: *const CGFloat,
        should_interpolate: Boolean,
        intent: u32,
    ) -> CGImageRef;
    pub fn CGImageGetWidth(image: CGImageRef) -> usize;
    pub fn CGImageGetHeight(image: CGImageRef) -> usize;
    pub fn CGImageCreateWithImageInRect(image: CGImageRef, rect: CGRectFFI) -> CGImageRef;
    pub fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: CGColorSpaceRef,
        bitmap_info: u32,
    ) -> CGContextRef;
    pub fn CGContextDrawImage(ctx: CGContextRef, rect: CGRectFFI, image: CGImageRef);
    pub fn CGContextFillPath(ctx: CGContextRef);
    pub fn CGContextStrokePath(ctx: CGContextRef);
    pub fn CGContextAddPath(ctx: CGContextRef, path: CGPathRef);
    pub fn CGContextSetFillColorWithColor(ctx: CGContextRef, color: CGColorRef);
    pub fn CGContextSetStrokeColorWithColor(ctx: CGContextRef, color: CGColorRef);
    pub fn CGContextSetLineWidth(ctx: CGContextRef, width: CGFloat);
    pub fn CGContextStrokeRect(ctx: CGContextRef, rect: CGRectFFI);
    pub fn CGContextFillRect(ctx: CGContextRef, rect: CGRectFFI);
    pub fn CGBitmapContextCreateImage(ctx: CGContextRef) -> CGImageRef;
    pub fn CGPathCreateWithRoundedRect(
        rect: CGRectFFI,
        corner_width: CGFloat,
        corner_height: CGFloat,
        transform: *const c_void,
    ) -> CGPathRef;
    pub fn CGPathCreateMutable() -> CGMutablePathRef;
    pub fn CGPathAddRect(path: CGMutablePathRef, transform: *const c_void, rect: CGRectFFI);
    pub fn CGPathAddRoundedRect(
        path: CGMutablePathRef,
        transform: *const c_void,
        rect: CGRectFFI,
        corner_width: CGFloat,
        corner_height: CGFloat,
    );
    pub fn CGColorCreate(rgb_color_space: CGColorSpaceRef, components: *const CGFloat) -> CGColorRef;
    pub fn CGImageDestinationCreateWithURL(
        url: *const c_void,
        format: *const c_void,
        count: usize,
        options: *const c_void,
    ) -> CGImageDestinationRef;
    pub fn CGImageDestinationAddImage(dest: CGImageDestinationRef, image: CGImageRef, properties: *const c_void);
    pub fn CGImageDestinationFinalize(dest: CGImageDestinationRef) -> Boolean;

    // Context state and drawing helpers
    pub fn CGContextSaveGState(ctx: CGContextRef);
    pub fn CGContextRestoreGState(ctx: CGContextRef);
    pub fn CGContextSetTextPosition(ctx: CGContextRef, x: CGFloat, y: CGFloat);
    pub fn CGContextEOFillPath(ctx: CGContextRef);
    pub fn CGBitmapContextGetHeight(ctx: CGContextRef) -> usize;
    pub fn CGBitmapContextGetWidth(ctx: CGContextRef) -> usize;
    pub fn CGContextMoveToPoint(ctx: CGContextRef, x: CGFloat, y: CGFloat);
    pub fn CGContextAddLineToPoint(ctx: CGContextRef, x: CGFloat, y: CGFloat);

    // Null rect for full-screen capture
    pub static kCGNullRect: CGRectFFI;
}

// ---------------------------------------------------------------------------
// CoreFoundation helper functions
// ---------------------------------------------------------------------------

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFArrayCreate(
        allocator: *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        callbacks: *const CFArrayCallBacks,
    ) -> CFArrayRef;
    pub fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    pub fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: CFIndex) -> *const c_void;
    pub fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
    pub fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: CFIndex,
        key_callbacks: *const CFDictionaryKeyCallBacks,
        value_callbacks: *const CFDictionaryValueCallBacks,
    ) -> CFDictionaryRef;
    pub fn CFStringGetCStringPtr(string: CFStringRef, encoding: u32) -> *const i8;
    pub fn CFStringGetCString(
        string: CFStringRef,
        buffer: *mut std::ffi::c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> bool;
    pub fn CFStringGetLength(string: CFStringRef) -> CFIndex;
    pub fn CFNumberGetValue(number: CFNumberRef, the_type: u32, value_ptr: *mut c_void) -> Boolean;
    pub fn CFBooleanGetValue(boolean: CFBooleanRef) -> Boolean;
    pub fn CFDataGetLength(data: CFDataRef) -> CFIndex;
    pub fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
    pub fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    pub fn CFStringGetTypeID() -> CFTypeID;
    pub fn CFNumberGetTypeID() -> CFTypeID;
    pub fn CFBooleanGetTypeID() -> CFTypeID;
    pub fn CFArrayGetTypeID() -> CFTypeID;
    pub fn CFDictionaryGetTypeID() -> CFTypeID;
    pub fn CFDataGetTypeID() -> CFTypeID;

    // CGWindowListCopyWindowInfo keys (defined as CFStringRef globals)
    pub static kCGWindowNumber: CFStringRef;
    pub static kCGWindowOwnerName: CFStringRef;
    pub static kCGWindowOwnerPID: CFStringRef;
    pub static kCGWindowName: CFStringRef;
    pub static kCGWindowBounds: CFStringRef;

    // CFDictionary callback globals
    pub static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
    pub static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;

    // CFArray callback global
    pub static kCFTypeArrayCallBacks: CFArrayCallBacks;

    // CFBoolean constants
    pub static kCFBooleanTrue: CFTypeRef;
}

// CFNumber type constants
pub const K_CF_NUMBER_SINT32_TYPE: u32 = 3;
pub const K_CF_NUMBER_DOUBLE_TYPE: u32 = 6;
pub const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

// Null singleton
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFNull: CFTypeRef;
}

// ---------------------------------------------------------------------------
// CoreText - Font + Line rendering (for annotation text)
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug)]
pub struct CTFont(c_void);

pub type CTFontRef = *const CTFont;

#[repr(C)]
#[derive(Debug)]
pub struct CTLine(c_void);

pub type CTLineRef = *const CTLine;

#[repr(C)]
#[derive(Debug)]
pub struct CFAttributedString(c_void);

pub type CFAttributedStringRef = *const CFAttributedString;

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    pub fn CTFontCreateWithName(
        name: CFStringRef,
        size: CGFloat,
        matrix: *const c_void,
    ) -> CTFontRef;
    pub fn CTLineCreateWithAttributedString(string: CFAttributedStringRef) -> CTLineRef;
    pub fn CTLineGetBoundsWithOptions(line: CTLineRef, options: u32) -> CGRectFFI;
    pub fn CTLineDraw(line: CTLineRef, context: CGContextRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFAttributedStringCreate(
        allocator: *const c_void,
        string: CFStringRef,
        attributes: CFDictionaryRef,
    ) -> CFAttributedStringRef;
}

// CoreText attribute key constants (defined as CFStringRef globals)
#[link(name = "CoreText", kind = "framework")]
extern "C" {
    pub static kCTFontAttributeName: CFStringRef;
    pub static kCTForegroundColorAttributeName: CFStringRef;
}
