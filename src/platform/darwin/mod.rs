//! macOS DarwinProvider backend.
//!
//! Implements the `DesktopProvider` trait using:
//! - AXUIElement (ApplicationServices) for accessibility tree walking
//! - CGEvent (CoreGraphics) for mouse/keyboard input simulation
//! - CGWindowList (CoreGraphics) for window enumeration
//! - NSWorkspace (AppKit) for application listing
//! - Vision framework for OCR
//! - `screencapture` CLI for screenshots

#[allow(dead_code)]
mod ffi;

pub mod key_code;

// TODO: implement each module
// mod app;
// mod snapshot;
// mod input;
// mod screenshot;
// mod ocr;
// mod annotation;
// mod saliency;

use crate::platform::DesktopProvider;

/// macOS implementation of `DesktopProvider`.
pub struct DarwinProvider;

impl DarwinProvider {
    pub fn new() -> Self {
        Self
    }
}

// TODO: implement DesktopProvider for DarwinProvider
