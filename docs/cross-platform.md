# Cross-Platform Prospects

Research notes on what it would take to bring forepaw's capabilities to Linux
and Windows. Written April 2026, updated with VM test results; the landscape
is actively shifting, especially on Linux.

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

### Windows VM testing

Tested on Windows 11 25H2 ARM64 (build 10.0.26200) in UTM on Apple Silicon.
All four capabilities validated.

**UIA tree walking:** The managed API (`System.Windows.Automation` via
`UIAutomationClient` assembly) works from Windows PowerShell 5.1. Rich tree --
File Explorer exposed 142 elements including toolbar buttons (New, Cut, Copy,
Paste, Rename, Share, Delete, Sort, View), address bar, navigation pane, drive
items with sub-properties (Name, Space used, Available space), status bar.
Taskbar exposed Start, Search, pinned apps, system tray, Clock. Comparable
quality to macOS AX.

**UIA actions:** `InvokePattern` (click), `ValuePattern` (set text), `TogglePattern`,
`ExpandCollapsePattern` all queryable per-element via `GetCurrentPattern()`.
Not yet tested interactively -- scripts ready for next session.

**Screen capture:** `System.Drawing.Graphics.CopyFromScreen` captures the screen
without permissions dialogs. Caveat: DPI scaling needs handling. At 175% scaling,
the captured image (1482x883) covers only a portion of the logical desktop
(~2498x1546). The Windows Graphics Capture API (Win10 1803+) handles DPI
correctly and can target specific windows.

**OCR:** `Windows.Media.Ocr` works from Windows PowerShell 5.1 in the interactive
session. No MSIX packaging, no package identity, no special setup needed. The
WinRT types load via `Add-Type` + an await helper. Produces per-line and per-word
results with bounding boxes. 25 lines extracted from a File Explorer screenshot
including navigation pane items, drive info ("28.4 GB free of 63.0 GB"), toolbar
labels, address bar text. Quality comparable to macOS Vision framework for UI text.

OneOCR (Snipping Tool engine) was **not available** on the test VM (v11.2307
doesn't ship the DLLs). Confirms the expiration risk from the research phase.
`Windows.Media.Ocr` is the working baseline.

**Session isolation:** SSH runs in session 0 (no desktop). UIA, screen capture,
and OCR all require the interactive desktop session (session 1). For dev testing,
a scheduled task bridge works (runs command in session 1 via `/RU <user> /IT`).
For production, forepaw itself runs in the interactive session -- either launched
by the user from a terminal, or as a persistent background process.

**SSH gotcha:** The OpenSSH DefaultShell registry entry must point to PS5.1
(`C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe`). PS7 installed via
winget/AppX is on the user PATH but not the SYSTEM PATH -- sshd runs as SYSTEM
and can't find `pwsh.exe`. A bare `pwsh.exe` or version-specific AppX path will
break after PS7 updates or service restarts.

**Still untested:** Chromium/Electron UIA quality, UIA actions (click/type on
real elements), per-window screenshots with DPI handling, Java/Swing testing.


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

Conceptually similar to macOS AX. Roles map well -- tested on KDE Plasma 6
(see [VM testing](#linux-vm-testing) below). Bounds are available in desktop
coordinates. Actions are named strings like macOS.

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
there's no way to flip a switch on an already-running Discord. Firefox
responds to the `MOZ_ENABLE_ACCESSIBILITY=1` environment variable, which
must also be set before launch.

**Activation:** AT-SPI2's bus exists by default on GNOME/KDE but
`org.a11y.Status.IsEnabled` defaults to `false`. Apps check this flag and
skip building their accessibility trees if it's off. Set
`org.gnome.desktop.interface.toolkit-accessibility = true` via
dconf/gsettings to enable it persistently. Without this, the bus is
running but empty.

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
screenshot/OCR workflow. KDE's `spectacle` CLI works without portal consent
when run within the graphical session (it's a trusted compositor client).
From SSH, importing the graphical session's environment variables
(`WAYLAND_DISPLAY`, `XDG_SESSION_TYPE`, etc.) from a compositor process
like `kwin_wayland` makes this work. Not elegant, but functional.

GNOME is harder: `gnome-screenshot` is deprecated and doesn't work on
GNOME Wayland (falls back to X11 which fails). The `org.gnome.Shell.Screenshot`
D-Bus API exists (`Screenshot`, `ScreenshotWindow`, `ScreenshotArea` methods)
but is access-controlled -- returns "Screenshot is not allowed" for non-shell
processes, including SSH sessions with correct `DBUS_SESSION_BUS_ADDRESS`.
The XDG Desktop Portal `Screenshot` interface pops a user consent dialog
on first use, but after granting permission once, subsequent requests
succeed silently. The permission is stored in the XDG permission store
(`screenshot` table with `{'': ['yes']}`) and persists across reboots.
The portal saves screenshots to `~/Pictures/` with auto-incrementing
names and returns the URI in the response.
grim (wlroots screenshotter) doesn't work on GNOME (needs wlr-screencopy
protocol).

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

This was expected to be the biggest problem, but AT-SPI2 actions change the
picture significantly (see [VM testing](#linux-vm-testing) below). For the
majority of forepaw's use cases -- clicking buttons, menu items, toggles --
AT-SPI2 `doAction("Press")` works over D-Bus and bypasses the compositor
entirely. `ydotool` (kernel-level via `/dev/uinput`) covers the remaining
cases where raw mouse/keyboard input is needed, and works on any compositor
without special protocols.

### Window management

**X11:** `wmctrl`, `xdotool`, EWMH protocol. Works.

**Wayland:** No standard window management protocol. Compositors deliberately
don't expose window lists to clients. Some offer D-Bus interfaces
(`org.gnome.Shell.Eval` for GNOME scripting), but nothing portable.

### The Wayland situation

Wayland's design philosophy is hostile to the traditional desktop automation
approach (screen capture + input injection). But AT-SPI2 operates on a
different plane -- it's a D-Bus protocol, not a Wayland protocol, so it
bypasses compositor restrictions entirely. Tree walking, element queries,
and action invocation all work from any process with D-Bus access, including
SSH sessions.

The remaining Wayland friction is in screen capture (needs compositor-specific
tools or portal consent) and raw input injection (needs `ydotool` with
`/dev/uinput` access). Both are solvable with system configuration.

A next-generation accessibility architecture called **Newton** is being
developed by Matt Campbell (AccessKit lead, ex-Microsoft Narrator/UIA team),
funded by the Sovereign Tech Fund. It proposes a push-based model where
providers push full accessibility tree snapshots and incremental updates
to clients (like Chromium's internal architecture), with a Wayland protocol
for toolkit-to-compositor communication and D-Bus for compositor-to-AT.
Tree updates are synchronized with visual frames.

As of June 2024, Newton has a working prototype: Orca is functional with
GTK4 apps (Nautilus, Text Editor, Podcasts, Fractal) running in Flatpak
sandboxes without AT-SPI2 bus access. However, Newton itself is not upstream
yet.

The AccessKit backend was merged into GTK 4.18 (May 2025). On Linux, GTK4
still defaults to AT-SPI2, but the AccessKit backend can be enabled with
`GTK_A11Y=accesskit`. NixOS's GTK4 package (4.20.3) has AccessKit
referenced but disabled at build time. Enabling it requires packaging
`accesskit-c` (a Rust crate producing C bindings, not yet in nixpkgs)
and overriding GTK4 with `-Daccesskit=enabled`, triggering a full GNOME
rebuild. When enabled, AccessKit's AT-SPI2 bridge may
produce richer trees than GTK4's native AT-SPI2 backend, since AccessKit's
model is based on Chromium's internal a11y (which exposes everything).
This is untested.

([Design doc](https://gnome.pages.gitlab.gnome.org/at-spi2-core/devel-docs/new-protocol.html),
[Newton update](https://blogs.gnome.org/a11y/2024/06/18/update-on-newton-the-wayland-native-accessibility-project/),
[GTK AccessKit merge](https://blogs.gnome.org/gtk/2025/05/12/an-accessibility-update/))

### Verdict

Linux is harder than macOS or Windows but more viable than initially expected.
AT-SPI2 actions are the key insight -- they handle the common case (clicking
UI elements) without needing compositor cooperation. Combined with `ydotool`
for raw input and compositor-native screenshot tools, all four capabilities
work on KDE Wayland today.

Remaining unknowns:
- Electron/Chromium app tree quality with `--force-renderer-accessibility`


### Linux VM testing

Tested on NixOS (aarch64) in UTM on Apple Silicon, KDE Plasma 6 Wayland
session. All four capabilities validated.

**AT-SPI2 tree walking:** 15 apps visible on the desktop. Kate editor
exposed 849 elements at depth 8 -- rich tree including menu bar with all
items, toolbar buttons, editor panels, status bar. plasmashell exposed the
full system tray, calendar widget (every day as a named button), and
clipboard popup.

Role mapping from AT-SPI2 to forepaw's model:

| AT-SPI2 | forepaw (macOS AX) | Notes |
|---------|-------------------|-------|
| application | AXApplication | direct |
| frame | AXWindow | direct |
| push button / button | AXButton | AT-SPI2 uses both names |
| menu bar, menu item | AXMenuBar, AXMenuItem | direct |
| text | AXTextField / AXTextArea | direct |
| check box | AXCheckBox | direct |
| panel | AXGroup | direct |
| label | AXStaticText | direct |
| page tab, page tab list | AXTab, AXTabGroup | direct |
| heading | *(no macOS equivalent)* | HTML semantics |
| filler | *(no macOS equivalent)* | KDE spacers |
| layered pane | *(no macOS equivalent)* | KDE stacking containers |

**AT-SPI2 actions:** `doAction("Press")` on plasmashell's "Application
Launcher" button opened the app launcher -- from an SSH session. Actions go
through D-Bus, not the compositor's input path, so they bypass all Wayland
restrictions. This is the single biggest finding: for 90% of forepaw's use
cases (clicking buttons, menu items, toggles), AT-SPI2 actions are the right
path on Linux. Available action names observed: Press, SetFocus, ShowMenu,
Toggle, Increase, Decrease.

**Screen capture:** `spectacle -b -n -f -o <path>` captures fullscreen
screenshots on KDE. Must be run with the graphical session's environment
variables -- from SSH, import `WAYLAND_DISPLAY`, `XDG_SESSION_TYPE`, etc.
from `kwin_wayland`'s `/proc/<pid>/environ`. KWin's `ScreenShot2` D-Bus
API also exists (`CaptureActiveWindow`, `CaptureArea`, `CaptureScreen`) but
rejects non-authorized (non-KDE) callers.

**Input injection:** `ydotool` with `ydotoold` daemon works. Needs
`/dev/uinput` access (ydotool group or root). NixOS has a
`programs.ydotool.enable` module that handles the systemd service, group,
and permissions. Tested mouse moves and text typing -- input goes to
whatever has focus (kernel-level, compositor-agnostic).

**OCR:** Tesseract via pytesseract reads text from spectacle screenshots.
20 text lines extracted from a Kate window capture.

**Setup requirements** (NixOS-specific, other distros will differ):
- `services.gnome.at-spi2-core.enable = true` (AT-SPI2 D-Bus service)
- `toolkit-accessibility = true` in dconf (enables `IsEnabled` on AT-SPI2 bus)
- `programs.ydotool.enable = true` (input injection daemon + permissions)
- `GI_TYPELIB_PATH` env var pointing to system typelibs (NixOS gotcha --
  typelibs exist but the search path isn't set by default)
- `MOZ_ENABLE_ACCESSIBILITY=1` (Firefox accessibility tree)

**KDE X11 session:** Also tested. AT-SPI2 tree identical to Wayland (849
elements in Kate, same tree structure). X11 has two advantages for
screenshot capture:
1. `magick import -window root` captures fullscreen without session env
   workarounds -- standard X11 capture Just Works from SSH.
2. Per-window capture via X11 window ID: `magick import -window 0x3a00017`
   grabs just one window, equivalent to macOS's `screencapture -l`. Window
   IDs available via `xprop -root _NET_CLIENT_LIST_STACKING`. Not available
   on Wayland.

18 apps visible on X11 (3 more than Wayland: `kscreen_backend_launcher`,
`kglobalaccel`, `baloorunner`). ydotool also works (kernel-level,
display-server-agnostic).

AT-SPI2 bounds can also be used for per-window crop on either display
server: capture fullscreen, crop to the `frame` element's extents. Less
clean than X11 window ID capture (includes overlapping windows) but works
on Wayland.

**Firefox on Linux:** Rich accessibility tree with `MOZ_ENABLE_ACCESSIBILITY=1`.
217 total elements. Full web content exposed: headings, links with `jump`
actions, paragraphs, landmarks, lists, buttons, checkboxes, form controls.
58 named links on the Privacy Notice page alone, all invokable via AT-SPI2
`doAction("jump")`. Tab switching works via `switch` action on `page tab`
elements. URL bar exposes `EditableText` interface for text input.

Tested pressing "Skip this step" button from SSH -- navigated Firefox's
onboarding wizard. Firefox's tree quality on Linux is excellent, comparable
to what forepaw gets from native macOS apps via AX.

**Still untested:** Electron/Chromium apps with
`--force-renderer-accessibility`, multi-monitor.


### GNOME Wayland testing

Tested on NixOS (aarch64) in UTM, GNOME 49 Wayland session. Results are
mixed -- AT-SPI2 works but GTK4 tree quality is poor, and screenshots
have no non-interactive path.

**AT-SPI2 tree walking:** The bus is active with `toolkit-accessibility = true`.
However, GTK4 apps expose dramatically sparse trees compared to KDE/Qt:

| App | Elements | Notes |
|-----|----------|-------|
| gnome-calculator | 25 | No individual buttons (0-9, +, -, etc.) |
| Nautilus (Files) | 10 | No file entries, no sidebar items |
| gnome-text-editor | 24 | No toolbar buttons, no menu items |
| GNOME Settings | 12 | No settings categories, no list items |
| gnome-shell | 120 | Mostly unnamed 0x0 panels |
| Firefox | 70 | Full tree with named buttons and actions |

GTK4 apps expose GAction names as AT-SPI2 actions on container elements
(e.g., `cal.solve`, `cal.clear`, `win.close`) rather than individual UI
elements with their own actions. The Calculator's button grid panel has
8 GActions but no child button elements. This means forepaw can invoke
application-level actions (save, close, preferences) but cannot target
individual UI widgets like number buttons or list items.

Firefox is the exception -- it implements its own AT-SPI2 backend (not via
GTK4's bridge) and exposes a full tree with 20+ named buttons, menu items,
tabs, and per-element press/click/switch actions. Same quality as on KDE.

This is a fundamental architectural difference: GTK4's accessibility model
is based on WAI-ARIA roles and a declarative `GtkAccessible` interface,
where widgets opt in to exposing children. Many GTK4 widgets don't expose
their internal structure to AT-SPI2. Qt widgets expose their full widget
tree by default.

The AccessKit backend (`GTK_A11Y=accesskit`) may produce richer trees, as
it uses Chromium-inspired semantics that expose more widget structure. This
backend was merged in GTK 4.18 but is disabled in the NixOS GTK4 package;
testing requires a package overlay with `-Daccesskit=enabled`.

**AT-SPI2 actions:** Work from SSH, same as KDE. Firefox button press
tested successfully. GTK4 GActions (invoked via `doAction` on the
container that exposes them) also return success.

**Screen capture:** GNOME Shell's D-Bus Screenshot API (`org.gnome.Shell.Screenshot`)
rejects non-shell callers ("Screenshot is not allowed"). gnome-screenshot
is deprecated and fails on Wayland. grim requires wlr-screencopy (not on
GNOME). The XDG Desktop Portal `Screenshot` interface works but requires
user consent on first use -- after clicking "Allow" once, subsequent
requests succeed silently. Permission persists in the XDG permission store
across reboots (one-time grant, similar to macOS Screen Recording permission).
Screenshots save to `~/Pictures/` with auto-incrementing names; the URI
is returned in the portal response.

**Input injection:** ydotool works (kernel-level, compositor-agnostic).
Mouse moves, clicks, and text typing all work from SSH.

**Clipboard:** spice-vdagent handles clipboard sharing at the SPICE
protocol level -- works across both KDE and GNOME sessions without
compositor-specific tools (no Klipper needed on GNOME).

**Implications for forepaw on GNOME:** The sparse GTK4 trees make
accessibility-tree-based automation much less useful for native GNOME apps
than for KDE/Qt apps. A GNOME forepaw implementation would need to lean
heavily on OCR/vision for GTK4 apps, while AT-SPI2 would still work well
for Firefox/Chromium (which have their own AT-SPI2 backends). This is a
significant finding -- the quality of automation depends on the desktop
environment and toolkit, not just the display server.


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
| Press/click action | AXPress / CGEvent | InvokePattern | Action "press"/"jump"/"switch" |
| Text input | AXSetValue / CGEvent keys | ValuePattern.SetValue / SendInput | EditableText.setTextContents / ydotool |
| Screenshot | screencapture -l | Graphics Capture API | spectacle/gnome-screenshot (Wayland) / scrot (X11) |
| Mouse events | CGEvent | SendInput | AT-SPI2 actions (primary) / ydotool (fallback) |
| Keyboard events | CGEvent | SendInput | ydotool / xdotool (X11) |

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
  Recording permissions. Windows has no equivalent gates. Linux needs
  uinput group for ydotool; AT-SPI2 and screenshot tools work without
  special permissions when configured correctly.


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

**No recommendation yet.** The decision depends on whether cross-platform
demand materializes, and whether forepaw's value is in the specific
implementation or in the CLI interface contract (command names, output
format, ref system) that agents learn once and use everywhere.

The language question matters most if targeting both Linux and Windows. Rust
has proven ecosystem support on both (atspi + zbus for Linux, windows-rs for
Windows UIA). Swift works on Linux but has no viable AT-SPI2 or UIA story.
Python has the best prototyping story (pyatspi2 for Linux, pywinauto for
Windows) but distribution and performance are concerns.

More VM testing will help inform this:
- Linux: GNOME Wayland, KDE X11, Firefox/Chromium tree quality
- Windows: UIA tree walking, input injection, Chromium native UIA
- Rust: prototype AT-SPI2 tree walk with `atspi` crate, compare with
  pyatspi2 for ergonomics and performance


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
