# Cross-Platform Prospects

Research notes on what it would take to bring forepaw's capabilities to Linux
and Windows. Written April 2026; the landscape is actively shifting,
especially on Linux.

forepaw needs four things from a platform:

1. **Accessibility tree** -- walk the UI element hierarchy, read roles/names/bounds,
   perform actions (press, set value)
2. **Screen capture** -- grab window screenshots for OCR and annotated screenshots
3. **Input injection** -- synthesize keyboard and mouse events
4. **Window management** -- enumerate windows, activate/focus apps

macOS provides all four through a coherent set of APIs (`AXUIElement`,
`screencapture`, `CGEvent`, `NSRunningApplication`). The other platforms are
more fragmented.

## Windows

### Accessibility: UI Automation (UIA)

Windows has the strongest accessibility API of the three platforms. UI
Automation (UIA) is the modern successor to MSAA (Microsoft Active
Accessibility), shipping since Windows Vista and actively maintained.

UIA provides:
- Tree walking via `IUIAutomationTreeWalker`
- Element properties: name, role (`ControlType`), bounding rectangle, value
- Control patterns: `InvokePattern` (click), `ValuePattern` (set text),
  `ScrollPattern`, `SelectionPattern`, `TextPattern` (rich text access)
- Condition-based search (`FindFirst`, `FindAll`) -- faster than walking for
  known targets
- Event subscriptions (structure changed, focus changed, property changed)

The pattern system is richer than macOS AX. Where macOS has a flat
`AXPerformAction("AXPress")`, UIA has typed patterns -- a scroll element
exposes `ScrollPattern` with `Scroll()`, `SetScrollPercent()`, etc. Capabilities
are queryable, not just trial-and-error.

**Chromium/Electron on Windows:** Chrome 138+ (mid-2025) enables native UIA
support by default. Chromium-based apps expose their accessibility trees
through UIA without any equivalent of `AXManualAccessibility` -- it just works.
This is a significant advantage over macOS, where Electron apps need the
attribute set explicitly.
([Chrome Developers blog](https://developer.chrome.com/blog/windows-uia-support-update))

**The COM problem:** UIA is a COM API. Calling it from Swift or Rust means COM
interop. From Rust, `windows-rs` handles this well -- Microsoft maintains
the crate and it has first-class UIA bindings
([sample](https://github.com/microsoft/windows-rs/blob/master/crates/samples/windows/uiautomation/src/main.rs)).
There's also `uiautomation-rs`, a higher-level wrapper
([docs.rs](https://docs.rs/uiautomation/latest/uiautomation/)). From Swift,
COM interop is possible but not idiomatic; nobody in the UIA ecosystem uses
Swift.

### Screen capture

`BitBlt`, `PrintWindow`, or the Windows Graphics Capture API. All work without
user consent dialogs. The Graphics Capture API (Windows 10 1803+) is the
modern option and handles DPI scaling correctly.

### Input injection

`SendInput` for keyboard and mouse events. Well-supported, no special
permissions. Works globally (not per-app). Higher-level: `SetForegroundWindow`
for activation, then `SendInput` for events.

### Window management

`EnumWindows`, `FindWindow`, `SetForegroundWindow`. Mature, well-documented,
no permission issues.

### Prior art: UFO

Microsoft Research's UFO/UFO2/UFO3 is a desktop agent for Windows that does
essentially what forepaw does: hybrid UIA + vision perception, action
execution, the full observe-act loop. It validates the architecture -- UIA
tree primary, screenshot fallback for custom-rendered UIs.
([UFO2 paper](https://www.microsoft.com/en-us/research/publication/ufo2-the-desktop-agentos/),
[GitHub](https://github.com/microsoft/UFO/))

Also: `pywinauto` is a mature Python library wrapping UIA for test automation
([docs](https://pywinauto.readthedocs.io/en/latest/getting_started.html)).

### Verdict

Windows is the most viable second platform. UIA is comprehensive, input
injection and screen capture Just Work, and there's no permissions theater or
compositor fragmentation. The main friction is language choice (see below).


## Linux

### Accessibility: AT-SPI2 over D-Bus

AT-SPI (Assistive Technology Service Provider Interface) is the accessibility
API for free desktops. GTK and Qt both expose their widget trees through it.
The protocol runs on a dedicated D-Bus bus (not the session bus).

AT-SPI2 provides:
- Tree walking via `Accessible` interface (parent, children, role, name, description)
- Actions via `Action` interface (press, activate, etc.)
- Text access via `Text` interface (character/word/line retrieval, caret position)
- Value, selection, table interfaces
- Event notifications (focus, state change, text change)

Conceptually similar to macOS AX. Roles map reasonably well (`ROLE_PUSH_BUTTON`
~ `AXButton`, `ROLE_TEXT` ~ `AXTextField`, etc.). Bounds are available. Actions
are named strings like macOS.

**Tooling:**
- Python: `pyatspi2` -- the standard binding, used by Orca screen reader
  ([example](https://www.freedesktop.org/wiki/Accessibility/PyAtSpi2Example/))
- Rust: `atspi` crate -- async D-Bus bindings via `zbus`
  ([crates.io](https://crates.io/crates/atspi)). Used by Odilia, a new Rust
  screen reader.
- C: `libatspi` in `at-spi2-core`
  ([GNOME](https://github.com/GNOME/at-spi2-core))

**Electron/Chromium on Linux:** Chromium uses AT-SPI2 on Linux. The
`--force-renderer-accessibility` flag forces the tree to be built. Unlike
macOS's `AXManualAccessibility` (set at runtime on a running process), this
flag must be set at app launch time. Detecting and handling this is harder --
there's no way to flip a switch on an already-running Discord.

### Screen capture

**X11:** `xdotool`, `scrot`, `import` (ImageMagick), `xwd`. All work without
permissions. Straightforward.

**Wayland:** This is where it falls apart. Wayland's security model forbids
clients from accessing other windows' content. Screen capture goes through
XDG Desktop Portal, which pops a user consent dialog. There is no silent
per-window capture equivalent to `screencapture -l <windowID>`.

Some compositors offer non-standard protocols:
- `wlr-screencopy` (wlroots-based: Sway, Hyprland) -- but not on GNOME/KDE
- `org.gnome.Shell.Screenshot` -- GNOME-specific D-Bus interface
- PipeWire-based screen capture via portal -- requires user interaction

For a CLI automation tool, the consent dialog is a dealbreaker for the
screenshot/OCR workflow. The options are compositor-specific backends or
accepting that screenshots require manual approval.

### Input injection

**X11:** `xdotool`, `xte`, XTest extension. Works globally, no permissions.

**Wayland:** No standard input injection protocol. Options:
- XDG Desktop Portal `RemoteDesktop` interface -- requires user consent, designed
  for remote desktop scenarios, not automation
- `wtype` (wlroots only) -- Wayland equivalent of `xdotool type`, but only for
  wlroots compositors
- `ydotool` -- uses `/dev/uinput` (kernel-level), works on any compositor but
  requires root or `uinput` group membership. Coordinates are absolute, no
  window-relative input.
- Compositor-specific: GNOME has an `InputCapture` portal (2025+), KDE has its
  own mechanisms

This is the single biggest problem for forepaw on Linux. Each compositor is
its own island.

### Window management

**X11:** `wmctrl`, `xdotool`, EWMH protocol. Works.

**Wayland:** No standard window management protocol. Compositors deliberately
don't expose window lists to clients. Some offer D-Bus interfaces
(`org.gnome.Shell.Eval` for GNOME scripting), but nothing portable.

### The Wayland situation

Wayland's design philosophy is explicitly hostile to desktop automation. From
the Wayland perspective, "no app should be able to see or control another
app's windows" is a security feature, not a bug. The accessibility community
has been pushing back on this -- Orca (the Linux screen reader) still has
significant Wayland issues as of 2026. Key capture, input injection, and
global UI inspection are all active areas of work.

A next-generation accessibility architecture is being designed by GNOME/AccessKit
contributors that would address some of these problems. It proposes a push-based
model (providers push tree snapshots to clients, like Chromium's internal
architecture) with Wayland integration. This would solve the tree-walking
problem properly but is still in the design/prototype phase.
([Design doc](https://gnome.pages.gitlab.gnome.org/at-spi2-core/devel-docs/new-protocol.html))

### Verdict

Linux is the hardest platform. AT-SPI2 covers the accessibility tree, but
everything else is fragmented across compositors. A practical Linux port would
probably need to:

1. Target GNOME specifically (largest desktop, most accessibility investment)
2. Accept X11-only for full functionality, with degraded Wayland support
3. Or target a specific Wayland compositor (Sway/Hyprland via wlroots protocols)

None of these are great options for a tool meant to Just Work.


## Language considerations

forepaw is currently Swift. The language choice was made for macOS-first
development and can change if cross-platform becomes a priority. How does
each language option look?

### Swift on Linux

Officially supported since Swift 5.x, with expanded support in Swift 6.
Foundation is cross-platform (the `swift-foundation` rewrite). SPM works.
SourceKit-LSP works. The toolchain installs cleanly.

For the ForepawCore target (platform-agnostic types, tree rendering, ref
assignment, annotation data model), Swift on Linux works fine today.

For a Linux platform backend, Swift would need D-Bus bindings to talk to
AT-SPI2. A few Swift D-Bus libraries exist
([PureSwift/DBus](https://github.com/PureSwift/DBus),
[wendylabsinc/dbus](https://github.com/wendylabsinc/dbus),
[subpop/swift-dbus](https://github.com/subpop/swift-dbus)) but all are
early-stage with minimal adoption (0-13 stars each). None have been used
for AT-SPI2. Compare with Rust's `zbus` (used by Odilia, AccessKit, and
the GNOME ecosystem) or Python's `pyatspi2` (used by Orca). Building
an AT-SPI2 client in Swift would mean layering the accessibility protocol
on top of immature D-Bus bindings -- two layers of unproven code.

### Swift on Windows

Officially supported but rougher than Linux. Requires Visual Studio C++
components. Some ecosystem gaps (SwiftNIO Windows support still in progress
as of mid-2025). Foundation and SPM work.

For a Windows platform backend, Swift would need COM interop for UIA. Swift
can call C/C++ but COM is verbose and there's no Swift-native UIA wrapper.
compnerd's `swift-win32` shows Win32 API wrapping in Swift is possible, but
UIA specifically hasn't been done.

### Alternative: Rust

Rust has the best cross-platform story for this kind of tool:

- **macOS:** `objc2` crate for Objective-C interop, `core-graphics` crate for
  CGEvent. Active ecosystem.
- **Windows:** `windows-rs` (Microsoft-maintained) with first-class UIA
  bindings. The `uiautomation` crate provides a higher-level wrapper. This is
  the strongest platform story.
- **Linux:** `atspi` crate for AT-SPI2 via `zbus`. Used by the Odilia screen
  reader. Active development.

[AccessKit](https://github.com/AccessKit/accesskit) is particularly relevant
-- it's a Rust library providing cross-platform accessibility infrastructure,
with adapters for macOS, Windows (UIA), and Linux (AT-SPI2). Its schema is
based on Chromium's internal accessibility model. GTK has merged
an AccessKit backend
([MR !8036](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/8036))
used on Windows and macOS, with Linux still defaulting to AT-SPI directly.
COSMIC's toolkit (iced) uses AccessKit. It's designed for *providing*
accessibility (toolkit side), not *consuming* it (AT side), but its schema
and platform adapters demonstrate that the cross-platform abstraction works.

### Alternative: Python

The pragmatic choice for maximum platform coverage with minimum friction:

- **macOS:** `pyobjc` for AX APIs, or shell out to forepaw itself
- **Windows:** `pywinauto` (mature, well-documented UIA wrapper)
- **Linux:** `pyatspi2` (the standard binding, used by Orca)

Downsides: performance (tree walks are IPC-heavy), distribution (bundling a
Python runtime), and forepaw's current architecture doesn't translate (Swift
protocols, value types, strict concurrency).


## Mapping forepaw's architecture

forepaw already has the right split for cross-platform:

| Layer | Platform-agnostic? | Notes |
|-------|-------------------|-------|
| `ForepawCore` | Yes | Types, protocols, tree rendering, ref system, annotations |
| `ForepawDarwin` | No | AX API, CGEvent, Vision, CoreGraphics |
| `Forepaw` (CLI) | Yes | ArgumentParser, output formatting |

A cross-platform port would add `ForepawWindows` and/or `ForepawLinux`
implementing `DesktopProvider`. The CLI and core logic stay the same.

The `DesktopProvider` protocol already uses platform-agnostic types (`Point`,
`Rect`, not `CGPoint`, `CGRect`). Element roles are strings. Actions are
string-identified. This was designed for exactly this scenario.

### What maps cleanly

| forepaw concept | macOS | Windows | Linux |
|----------------|-------|---------|-------|
| Tree walking | AXUIElement children | UIA TreeWalker | AT-SPI2 Accessible children |
| Element role | AXRole string | ControlType enum | AT-SPI2 Role enum |
| Element name | AXTitle/AXDescription | Name property | Name/Description |
| Bounds | AXPosition + AXSize | BoundingRectangle | Extents |
| Press/click action | AXPress / CGEvent | InvokePattern | Action "press" |
| Text input | AXSetValue / CGEvent keys | ValuePattern.SetValue / SendInput | Text.SetCaretOffset + keys |
| Screenshot | screencapture -l | Graphics Capture API | Portal (Wayland) / X11 capture |
| Mouse events | CGEvent | SendInput | ydotool / xdotool / wtype |
| Keyboard events | CGEvent | SendInput | ydotool / xdotool / wtype |

### What doesn't map

- **Electron detection and `AXManualAccessibility`**: macOS-only. Windows
  doesn't need it (UIA works natively on Chromium). Linux would need
  `--force-renderer-accessibility` at launch time.
- **OCR via Vision framework**: macOS-only. Windows has Windows.Media.Ocr.
  Linux has Tesseract. All produce similar results but with different APIs.
- **Annotated screenshots**: The `AnnotationRenderer` uses CoreGraphics.
  Would need platform-specific image drawing (GDI+/Direct2D on Windows,
  Cairo on Linux). Or use a cross-platform image library.
- **Permission model**: macOS requires explicit Accessibility and Screen
  Recording permissions. Windows has no equivalent gates. Linux varies
  (uinput group for ydotool, portal consent for Wayland capture).


## Realistic paths forward

**Option 1: Stay Swift, add platform backends in Swift.**
Keeps the codebase unified. ForepawCore and CLI stay as-is. Add
`ForepawWindows` (COM/UIA via C interop) and `ForepawLinux` (D-Bus/AT-SPI2
via C interop). Friction: COM from Swift is painful, with no existing UIA-from-Swift
precedent. D-Bus from Swift is equally uncharted.

**Option 2: Stay Swift for Core + CLI, write platform backends in C.**
ForepawCore compiles on all platforms. Platform backends are C libraries
exposing a flat C API that Swift calls via C interop. The C layer wraps
platform-native APIs (UIA COM on Windows, AT-SPI2 D-Bus on Linux). Downside:
maintaining C wrapper code, two-language build.

**Option 3: Rewrite in Rust.**
`windows-rs` for Windows, `atspi` for Linux, `objc2` for macOS. One language,
good platform interop on all three. The `clap` crate mirrors ArgumentParser's
subcommand pattern. The codebase is small enough that a rewrite is feasible. Downside: losing Swift's macOS-native feel and the
existing working implementation.

**Option 4: Separate implementations per platform.**
forepaw (Swift/macOS), forepaw-win (Rust or C#/Windows), forepaw-linux
(Rust or Python/Linux). Shared CLI interface and output format, different
internals. Each implementation is idiomatic for its platform. Downside:
three codebases to maintain, drift risk.

**No recommendation here.** The decision depends on whether cross-platform
demand materializes, and whether forepaw's value is in the specific
implementation or in the CLI interface contract (command names, output
format, ref system) that agents learn once and use everywhere.

VM-based testing on Windows and Linux can help validate feasibility before
committing to a path.


## References

- [AT-SPI2 core](https://github.com/GNOME/at-spi2-core) -- D-Bus interface
  definitions and daemons for Linux accessibility
- [AT-SPI2 developer guide](https://gnome.pages.gitlab.gnome.org/at-spi2-core/devel-docs/index.html)
- [Next-gen accessibility architecture proposal](https://gnome.pages.gitlab.gnome.org/at-spi2-core/devel-docs/new-protocol.html) --
  push-based model, Wayland integration, AccessKit schema
- [pyatspi2 tutorial](https://www.freedesktop.org/wiki/Accessibility/PyAtSpi2Example/)
- [Wayland accessibility notes](https://github.com/splondike/wayland-accessibility-notes)
- [Wayland development struggles (2026)](https://byteiota.com/wayland-development-why-developers-still-struggle-in-2026/)
- [Microsoft UI Automation overview](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-uiautomationoverview)
- [Chrome native UIA support (2025)](https://developer.chrome.com/blog/windows-uia-support-update)
- [UFO2: The Desktop AgentOS](https://www.microsoft.com/en-us/research/publication/ufo2-the-desktop-agentos/)
- [pywinauto](https://pywinauto.readthedocs.io/en/latest/getting_started.html)
- [windows-rs UIA sample](https://github.com/microsoft/windows-rs/blob/master/crates/samples/windows/uiautomation/src/main.rs)
- [uiautomation-rs](https://docs.rs/uiautomation/latest/uiautomation/)
- [atspi crate](https://crates.io/crates/atspi) -- Rust AT-SPI2 bindings
- [Odilia screen reader](https://www.reddit.com/r/rust/comments/10gbhx3/linux_assistive_technologies_in_rust_atspi_and/) -- Rust screen reader using atspi crate
- [AccessKit](https://github.com/AccessKit/accesskit) -- cross-platform
  accessibility infrastructure in Rust
- [Swift platform support](https://www.swift.org/platform-support/)
- [Swift on Windows install](https://www.swift.org/install/windows/)
- [swift-win32](https://github.com/compnerd/swift-win32) -- Win32 API wrapping in Swift
- [xdg-desktop-portal-generic](https://github.com/lamco-admin/xdg-desktop-portal-generic) --
  generic portal backend for Wayland screen capture and input
- [lamco_portal](https://docs.rs/lamco-portal/latest/lamco_portal/) -- Rust
  XDG Desktop Portal integration
