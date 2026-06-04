# forepaw

A raccoon's paws on your desktop, as a library.

Cross-platform desktop automation for Rust. Control any application through
accessibility trees, OCR, and input simulation. Platform-agnostic types and a
trait-based backend system: macOS uses AXUIElement + CoreGraphics, Windows uses
UI Automation + Win32, Linux uses AT-SPI2 + D-Bus.

Named after the raccoon's dexterous forepaws: precise manipulation of UI
elements without brute force. forepaw is the paws; the brain is whatever you
connect them to.

## Usage

```toml
[dependencies]
forepaw = "0.4"
```

```rust
use forepaw::platform::DesktopProvider;
use forepaw::platform::AppTarget;

// Pick the backend for the current platform
#[cfg(target_os = "macos")]
let provider = forepaw::platform::darwin::DarwinProvider::new();

#[cfg(target_os = "windows")]
let provider = forepaw::platform::windows::WindowsProvider::new();

#[cfg(target_os = "linux")]
let provider = forepaw::platform::linux::LinuxProvider::new();

let provider = &provider as &dyn DesktopProvider;

// List running apps
let apps = provider.list_apps()?;

// Read the accessibility tree
let tree = provider.snapshot(
    &AppTarget::name("Finder"),
    None,
    &Default::default(),
)?;

// Click an element by ref
provider.click_ref(element_ref, &AppTarget::name("Finder"), &Default::default())?;

// OCR
let ocr = provider.ocr(Some(&AppTarget::name("Notes")), None, None, None)?;
for result in &ocr.results {
    println!("{}: {:?}", result.text, result.bounds);
}
```

## What's in the box

- **`DesktopProvider` trait**: unified interface for observation (snapshot,
  screenshot, OCR, hit-test) and action (click, type, press, scroll, drag).
- **`ElementTree` / `ElementNode` / `ElementData`**: accessibility tree
  representation with role, name, value, bounds, element state (enabled,
  focused, selected), identifiers, and content signatures for cross-snapshot
  matching.
- **`Role` enum**: platform-agnostic roles mapped from AXRole (macOS),
  UIA ControlType (Windows), and AT-SPI2 Role (Linux).
- **`WindowState`**: window display state (normal, minimized, maximized,
  fullscreen) per platform.
- **Tree rendering, diffing, annotation**: format trees as text, diff
  before/after snapshots, annotate screenshots with element labels.
- **`RefAssigner`**: deterministic `@e1`, `@e2`, ... ref assignment with
  optional interactive-only filtering and content signature generation.
- **Logging**: zero-dep `FOREPAW_LOG` / `RUST_LOG` filtering with
  per-module levels.

## Platform support

| Platform | Backend | Status |
|----------|---------|--------|
| macOS 14+ | AXUIElement, CGEvent, Vision OCR | Full |
| Windows | UI Automation, Win32, WinRT OCR | Observation done, actions in progress |
| Linux | AT-SPI2, D-Bus | Observation mostly done |

## Cargo features

No cargo features. Platform selection is automatic via `cfg(target_os)`.

The `image` crate is only pulled in on Windows (for OCR upscaling). macOS uses
CoreGraphics directly, Linux has no image dependency.

## CLI

For the command-line tool, see the [`forepaw-cli`](https://github.com/aviraccoon/forepaw/tree/main/crates/forepaw-cli) crate.

## License

[Unlicense](https://unlicense.org/). Public domain. Raccoons don't believe in fences.
