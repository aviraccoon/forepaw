//! Error types for forepaw operations.
use std::fmt;

use crate::core::element_tree::ElementRef;

/// Errors that can occur during forepaw operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum ForepawError {
    /// The specified application is not running.
    AppNotFound(String),
    /// An element ref is no longer valid (snapshot is stale).
    StaleRef(ElementRef),
    /// A user-facing action (click, type, press) failed.
    ActionFailed(String),
    /// Accessibility permission has not been granted.
    PermissionDenied,
    /// Screen recording permission has not been granted.
    ScreenRecordingDenied,
    /// No window matched the given query.
    WindowNotFound(String),
    /// Multiple windows matched the given query.
    AmbiguousWindow {
        /// The ambiguous query string.
        query: String,
        /// Matching windows (formatted for display).
        matches: String,
    },
}

impl fmt::Display for ForepawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AppNotFound(name) => write!(
                f,
                "Application not found: {name}. Run 'forepaw list-apps' to see running apps."
            ),
            Self::StaleRef(ref_val) => {
                write!(
                    f,
                    "Stale ref: {ref_val}. Run 'forepaw snapshot' to refresh refs, then retry."
                )
            }
            Self::ActionFailed(msg) => write!(f, "Action failed: {msg}"),
            Self::PermissionDenied => {
                write!(
                    f,
                    "Accessibility permission not granted. Run 'forepaw permissions' to check."
                )
            }
            Self::ScreenRecordingDenied => {
                write!(
                    f,
                    "Screen recording permission not granted. Run 'forepaw permissions' to check."
                )
            }
            Self::WindowNotFound(query) => {
                write!(f, "Window not found: {query}. Run 'forepaw list-windows --app <name>' to see windows.")
            }
            Self::AmbiguousWindow { query, matches } => {
                write!(f, "Multiple windows match '{query}'. Use --window with a more specific title or window ID:\n{matches}")
            }
        }
    }
}

impl std::error::Error for ForepawError {}
