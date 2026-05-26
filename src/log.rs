//! Minimal logging with `FOREPAW_LOG` env var.
//!
//! No dependencies — just std. Supports the standard `RUST_LOG`-style
//! convention with per-module filtering.
//!
//! # Usage
//!
//! ```ignore
//! forepaw::log::init();
//! forepaw::info!("snapshot took {:.1}ms", 42.0);
//! ```
//!
//! # Environment variable format
//!
//! - `FOREPAW_LOG=debug` — global level
//! - `FOREPAW_LOG=snapshot=debug,app=info` — per-module overrides
//! - `FOREPAW_LOG=warn,snapshot=debug` — global level + override
//! - `RUST_LOG=info` — fallback if `FOREPAW_LOG` is unset
//!
//! Module names match against the start of the module path
//! (with `forepaw::` stripped). `snapshot=debug` matches both
//! `snapshot` and `snapshot::build_tree`.
//!
//! Valid levels (least to most verbose): `error`, `warn`, `info`, `debug`, `trace`.
//! Default: `warn` (shows errors and warnings).

// ---------------------------------------------------------------------------
// Level
// ---------------------------------------------------------------------------

/// Log levels ordered from least to most verbose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    fn parse(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Log config
// ---------------------------------------------------------------------------

/// Default level: show errors and warnings.
const DEFAULT_LEVEL: Level = Level::Warn;

/// A module-level override: a module path prefix and its effective level.
struct ModuleFilter {
    prefix: String,
    level: Level,
}

struct LogConfig {
    global: Level,
    modules: Vec<ModuleFilter>,
}

use std::sync::OnceLock;

static CONFIG: OnceLock<LogConfig> = OnceLock::new();

fn config() -> &'static LogConfig {
    CONFIG.get_or_init(|| {
        let val = std::env::var("FOREPAW_LOG")
            .or_else(|_| std::env::var("RUST_LOG"))
            .unwrap_or_default();
        parse_config(&val)
    })
}

/// Parse a `FOREPAW_LOG`-style value into a config.
fn parse_config(val: &str) -> LogConfig {
    let trimmed = val.trim();
    if trimmed.is_empty() {
        return LogConfig {
            global: DEFAULT_LEVEL,
            modules: Vec::new(),
        };
    }

    let parts: Vec<&str> = trimmed
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut modules = Vec::new();
    let mut global = DEFAULT_LEVEL;

    for part in &parts {
        if let Some((module, level_str)) = part.split_once('=') {
            let module = module.trim();
            let level_str = level_str.trim();
            if !module.is_empty() {
                if let Some(level) = Level::parse(level_str) {
                    modules.push(ModuleFilter {
                        prefix: module.to_owned(),
                        level,
                    });
                }
                continue;
            }
        }
        // No '=' or empty module name — treat as global level
        if let Some(level) = Level::parse(part) {
            global = level;
        }
    }

    LogConfig { global, modules }
}

/// Strip the `forepaw::` prefix from a `module_path!()` result.
/// Strip `forepaw::` and the platform-specific segment from a `module_path!()` result.
///
/// `forepaw::platform::darwin::snapshot` → `snapshot`
/// `forepaw::platform::windows::ocr` → `ocr`
/// `forepaw::core::element_tree` → `core::element_tree`
fn strip_crate_prefix(full: &str) -> &str {
    let s = full.strip_prefix("forepaw::").unwrap_or(full);
    // Strip platform::<os>:: prefix so `snapshot=debug` works across macOS/Windows/Linux.
    if let Some(rest) = s.strip_prefix("platform::") {
        if let Some((_, leaf)) = rest.split_once("::") {
            return leaf;
        }
    }
    s
}

/// Check whether messages at `level` for `module` would be printed.
///
/// `module` should be the result of `module_path!()` from the call site.
#[inline]
#[must_use]
pub fn enabled(level: Level, module: &str) -> bool {
    let cfg = config();
    let effective = module_level(cfg, module).unwrap_or(cfg.global);
    level as u8 <= effective as u8
}

/// Find the most specific module override for the given path.
fn module_level(cfg: &LogConfig, module: &str) -> Option<Level> {
    let stripped = strip_crate_prefix(module);
    // Check overrides: the longest matching prefix wins.
    cfg.modules
        .iter()
        .filter(|m| stripped.starts_with(&m.prefix))
        .max_by_key(|m| m.prefix.len())
        .map(|m| m.level)
}

// ---------------------------------------------------------------------------
// Logging macros
// ---------------------------------------------------------------------------

/// Log an error message.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Error, module_path!()) {
            eprintln!("[ERROR] {}", format_args!($($arg)*));
        }
    };
}

/// Log a warning message.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Warn, module_path!()) {
            eprintln!("[WARN]  {}", format_args!($($arg)*));
        }
    };
}

/// Log an informational message.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Info, module_path!()) {
            eprintln!("[INFO]  {}", format_args!($($arg)*));
        }
    };
}

/// Log a debug message.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Debug, module_path!()) {
            eprintln!("[DEBUG] {}", format_args!($($arg)*));
        }
    };
}

/// Log a trace message.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        if $crate::log::enabled($crate::log::Level::Trace, module_path!()) {
            eprintln!("[TRACE] {}", format_args!($($arg)*));
        }
    };
}

/// Initialize logging.
///
/// Reads the environment variable on first call; subsequent calls are no-ops.
pub fn init() {
    config();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_ordering() {
        assert!(Level::Error < Level::Warn);
        assert!(Level::Warn < Level::Info);
        assert!(Level::Info < Level::Debug);
        assert!(Level::Debug < Level::Trace);
    }

    #[test]
    fn parse_valid_levels() {
        assert_eq!(Level::parse("error"), Some(Level::Error));
        assert_eq!(Level::parse("warn"), Some(Level::Warn));
        assert_eq!(Level::parse("warning"), Some(Level::Warn));
        assert_eq!(Level::parse("info"), Some(Level::Info));
        assert_eq!(Level::parse("debug"), Some(Level::Debug));
        assert_eq!(Level::parse("trace"), Some(Level::Trace));
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(Level::parse("ERROR"), Some(Level::Error));
        assert_eq!(Level::parse("Debug"), Some(Level::Debug));
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert_eq!(Level::parse(""), None);
        assert_eq!(Level::parse("verbose"), None);
        assert_eq!(Level::parse("123"), None);
    }

    // --- Config parsing ---

    #[test]
    fn config_empty_defaults_to_warn() {
        let cfg = parse_config("");
        assert_eq!(cfg.global, Level::Warn);
        assert!(cfg.modules.is_empty());
    }

    #[test]
    fn config_global_level() {
        let cfg = parse_config("debug");
        assert_eq!(cfg.global, Level::Debug);
        assert!(cfg.modules.is_empty());
    }

    #[test]
    fn config_single_module() {
        let cfg = parse_config("snapshot=debug");
        assert_eq!(cfg.global, Level::Warn);
        assert_eq!(cfg.modules.len(), 1);
        assert_eq!(cfg.modules[0].prefix, "snapshot");
        assert_eq!(cfg.modules[0].level, Level::Debug);
    }

    #[test]
    fn config_global_with_module_override() {
        let cfg = parse_config("warn,snapshot=debug");
        assert_eq!(cfg.global, Level::Warn);
        assert_eq!(cfg.modules.len(), 1);
        assert_eq!(cfg.modules[0].prefix, "snapshot");
        assert_eq!(cfg.modules[0].level, Level::Debug);
    }

    #[test]
    fn config_multiple_modules() {
        let cfg = parse_config("snapshot=debug,app=info");
        assert_eq!(cfg.global, Level::Warn);
        assert_eq!(cfg.modules.len(), 2);
    }

    #[test]
    fn config_invalid_part_skipped() {
        let cfg = parse_config("debug,oops=invalid");
        assert_eq!(cfg.global, Level::Debug);
        assert!(cfg.modules.is_empty());
    }

    #[test]
    fn config_whitespace_around_parts() {
        let cfg = parse_config("  debug , snapshot = trace ");
        assert_eq!(cfg.global, Level::Debug);
        assert_eq!(cfg.modules.len(), 1);
        assert_eq!(cfg.modules[0].prefix, "snapshot");
        assert_eq!(cfg.modules[0].level, Level::Trace);
    }

    // --- Strip crate prefix ---

    #[test]
    fn strip_prefix_from_full_path() {
        assert_eq!(strip_crate_prefix("forepaw::snapshot"), "snapshot");
        assert_eq!(
            strip_crate_prefix("forepaw::platform::darwin::snapshot"),
            "snapshot"
        );
        assert_eq!(
            strip_crate_prefix("forepaw::platform::windows::ocr"),
            "ocr"
        );
        assert_eq!(
            strip_crate_prefix("forepaw::core::element_tree"),
            "core::element_tree"
        );
    }

    #[test]
    fn strip_prefix_noop_without_crate() {
        assert_eq!(strip_crate_prefix("snapshot"), "snapshot");
    }

    #[test]
    fn strip_prefix_platform_core_path() {
        // Platform paths get OS leaf stripped, core paths keep their segments.
        assert_eq!(
            strip_crate_prefix("forepaw::platform::darwin::app::find_window"),
            "app::find_window"
        );
    }

    // --- Module level resolution ---

    #[test]
    fn module_level_finds_prefix() {
        let cfg = parse_config("snapshot=debug");
        assert_eq!(
            module_level(&cfg, "forepaw::platform::darwin::snapshot::build_tree"),
            Some(Level::Debug)
        );
    }

    #[test]
    fn module_level_falls_back_to_none() {
        let cfg = parse_config("snapshot=debug");
        assert_eq!(module_level(&cfg, "forepaw::platform::darwin::app"), None);
    }

    #[test]
    fn module_level_longest_prefix_wins() {
        let cfg = parse_config("snapshot=info,snapshot::build=trace");
        assert_eq!(
            module_level(&cfg, "forepaw::platform::darwin::snapshot::build_tree"),
            Some(Level::Trace)
        );
        assert_eq!(
            module_level(&cfg, "forepaw::platform::darwin::snapshot::other"),
            Some(Level::Info)
        );
    }

    // --- enabled() uses CONFIG static which can't be reset across tests ---
    // Covered by parse_config + module_level + level comparison tests above.

    #[test]
    fn enabled_via_parse_and_module_level() {
        // Equivalent to FOREPAW_LOG=info
        let cfg = parse_config("info");
        assert!(module_level(&cfg, "anything").is_none());
        assert!(cfg.global == Level::Info);
        assert!(Level::Error as u8 <= Level::Info as u8);
        assert!(Level::Warn as u8 <= Level::Info as u8);
        assert!(Level::Info as u8 <= Level::Info as u8);
        assert!(!(Level::Debug as u8 <= Level::Info as u8));
        assert!(!(Level::Trace as u8 <= Level::Info as u8));
    }
}
