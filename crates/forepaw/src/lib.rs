//! A raccoon's paws on your desktop. Cross-platform automation library.
//!
//! Control any application through accessibility trees, OCR, and input simulation.
//! Defines platform-agnostic types, the [`DesktopProvider`](crate::platform::DesktopProvider) trait,
//! and platform backends for macOS, Windows, and Linux.
//!
//! forepaw lets programs interact with any desktop application the same way a
//! human would: reading what's on screen, clicking buttons, typing text, scrolling
//! around. On macOS it reads the accessibility tree that `VoiceOver` uses.
//! On Windows it uses UI Automation (the tree `Narrator` uses). On Linux it uses
//! `AT-SPI2` (the protocol `Orca` uses). Input simulation works through native
//! platform APIs on all three.

pub mod core;
pub mod log;
pub mod platform;
