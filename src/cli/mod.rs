/// CLI module: clap subcommands and shared utilities.
pub mod action;
pub mod observation;
pub mod parse;
pub mod system;

use crate::core::output_formatter::OutputFormat;
use crate::platform::{AppTarget, WindowTarget};

/// Global options parsed from the top-level `forepaw` command.
///
/// Passed to every subcommand's `run()` method.
#[derive(Debug, Clone, Copy)]
pub struct GlobalArgs {
    pub format: OutputFormat,
}

impl GlobalArgs {
    #[must_use]
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    #[must_use]
    pub fn json(&self) -> bool {
        self.format == OutputFormat::Json
    }
}

/// `--app` and `--pid` flags (mutually exclusive).
///
/// Flattened into observation commands and action structs.
/// Clap enforces that at most one is set.
#[derive(clap::Args, Clone, Debug)]
#[group(multiple = false)]
pub struct AppTargetArgs {
    #[arg(long, help = "Target application name")]
    pub app: Option<String>,

    #[arg(long, help = "Target application by process ID")]
    pub pid: Option<i32>,
}

impl AppTargetArgs {
    /// Resolve `--app` or `--pid` into an `AppTarget`.
    ///
    /// Returns `Ok(Some(target))` if one is set, `Ok(None)` if neither is set.
    /// Clap's group validation already prevents both being set.
    ///
    /// # Errors
    ///
    /// Returns an error only if clap's group validation was somehow bypassed.
    /// In practice, this should never happen.
    pub fn resolve(&self) -> anyhow::Result<Option<AppTarget>> {
        match (&self.app, &self.pid) {
            (Some(name), None) => Ok(Some(AppTarget::name(name))),
            (None, Some(pid)) => Ok(Some(AppTarget::pid(*pid))),
            (None, None) => Ok(None),
            (Some(_), Some(_)) => unreachable!("clap group prevents both --app and --pid"),
        }
    }

    /// Resolve `--app` or `--pid`, requiring exactly one.
    ///
    /// # Errors
    ///
    /// Returns an error if neither is provided.
    pub fn require(&self, context: &str) -> anyhow::Result<AppTarget> {
        self.resolve()?
            .ok_or_else(|| anyhow::anyhow!("--app or --pid is required for {context}"))
    }
}

/// `--window` and `--window-id` flags (mutually exclusive).
///
/// Flattened into observation commands and action structs.
/// Clap enforces that at most one is set.
#[derive(clap::Args, Clone, Debug)]
#[group(multiple = false)]
pub struct WindowTargetArgs {
    #[arg(long, help = "Window title (case-insensitive substring match)")]
    pub window: Option<String>,

    #[arg(long, help = "Window ID from list-windows (e.g. 7290)")]
    pub window_id: Option<String>,
}

impl WindowTargetArgs {
    /// Resolve `--window` or `--window-id` into a `WindowTarget`.
    ///
    /// Returns `Some(target)` if one is set, `None` if neither is set.
    /// Clap's group validation already prevents both being set.
    #[must_use]
    pub fn resolve(&self) -> Option<WindowTarget> {
        match (&self.window, &self.window_id) {
            (Some(title), None) => Some(WindowTarget::title(title)),
            (None, Some(id)) => Some(WindowTarget::id(id.strip_prefix("w-").unwrap_or(id))),
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("clap group prevents both --window and --window-id"),
        }
    }
}
