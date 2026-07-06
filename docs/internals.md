# Internals

How forepaw works under the hood. For performance numbers, see `performance-macos.md`.

## The accessibility tree

> **The raccoon version:** Imagine a raccoon feeling around a trash can in the dark. It doesn't see the lid, the bag, the food — it feels shapes, edges, handles. That's the accessibility tree. Every app on every OS exposes a tree of UI elements — buttons, text fields, labels — that screen readers (and raccoons like us) can feel around in without looking at pixels. Different dumpsters, same feeling.

Each operating system exposes a different tree API:

| OS | API | Element type | Access pattern |
|----|-----|-------------|----------------|
| macOS | Accessibility (AX) via IPC | `AXUIElement` | Batch attribute fetch (17 in one call) |
| Windows | UI Automation (UIA) via COM | `IUIAutomationElement` | Individual property calls |
| Linux | AT-SPI2 via D-Bus | Object path + bus name | Individual method calls over D-Bus |

The tree is walked depth-first with a configurable max depth (default 15, 25 for Electron apps). Each interactive element gets a positional ref (`@e1`, `@e2`, ...) based on its depth-first order.

### macOS: AXUIElement

`AXUIElementCreateApplication(pid)` gives a handle to an app's UI hierarchy. From there:

- `AXUIElementCopyMultipleAttributeValues` reads attributes in batch (17 per element in one IPC call)
- `AXUIElementCopyAttributeValue` reads individual attributes (only used for `AXTitleUIElement` targets, which are different `AXUIElement` handles)
- `AXUIElementPerformAction` performs actions (`AXPress`, `AXRaise`)
- `AXUIElementSetAttributeValue` sets values directly (`AXValue`, `AXFocused`, `AXManualAccessibility`)

### Windows: IUIAutomation

`IUIAutomation::GetRootElement` gets the desktop root. App elements are found via `IUIAutomation::GetRootElement` → `FindAll` with `TreeScope_Children`, or via `IUIAutomation::ElementFromHandle` using an HWND from `EnumWindows`. The `ControlViewWalker` provides a pre-filtered tree (structural containers excluded), closest to macOS's pruned AX tree. Each property (`CurrentName`, `CurrentControlType`, `CurrentBoundingRectangle`) is an individual COM call.

### Linux: AT-SPI2 via D-Bus

AT-SPI2 requires two D-Bus connections: the session bus (to discover the AT-SPI2 bus address via `org.a11y.Bus.GetAddress()`) and the AT-SPI2 bus itself (for all accessibility calls). The `zbus` crate provides blocking D-Bus access.

Each property read (`GetRole`, `GetName`, `GetExtents`, `GetChildren`, `GetState`) is an individual D-Bus method call. Qt apps (KDE) use per-app bus names on the AT-SPI2 bus. GTK apps (GNOME) share the registry bus. The tree walk detects this: if a child's bus name starts with `:` and differs from the app's bus, it's a separate Qt process's bus name.

### Batched attribute fetching

> **The raccoon version:** A raccoon could pick up one french fry at a time, or it could grab the whole container. We grab the whole container. One trip to the trash can per item instead of a couple dozen.

On macOS, `AXUIElementCopyMultipleAttributeValues` fetches the full batched attribute set per element in a single Mach IPC round-trip. The attribute list is declared once in `darwin/snapshot.rs` via a `define_attrs!` macro that emits both a `#[repr(usize)] Attr` enum and the matching `AX*` name slice from a single `(Variant, "AXName")` list — adding an attribute is one line, and the enum variant becomes the index used everywhere (`attrs.string(Attr::Title)`), so index constants can never drift from the name array.

The batch covers three groups:

- **Core tree structure & name resolution**: Role, Title, Description, Value, Position, Size, Children, Subrole, TitleUIElement, Help, PlaceholderValue, DOMClassList, RoleDescription.
- **Element state & identity**: Enabled, Focused, Selected, Identifier.
- **Extra context** (collected into each element's `attributes` bag by `collect_extra_attributes`, surfaced in verbose/JSON output): Orientation, Expanded, MinValue, MaxValue, ValueIncrement, URL, SortDirection, Index, Required, ElementBusy, DisclosureLevel, AccessKey, Filename.

Fetching the whole batch in one call avoids one IPC per attribute per element. The impact is dramatic on slow AX responders — Music.app went from ~50s to ~12s.

On Windows, each property is an individual COM call. A `CacheRequest` could batch them via `IUIAutomation::CreateCacheRequest` + `BuildUpdatedCache`, but that's not wired yet. On Linux, each property is an individual D-Bus call — no batch equivalent exists.

### Element name resolution

> **The raccoon version:** Not everything in the dumpster is labeled. Sometimes there's a tag on the container, sometimes you have to open it and sniff the contents, sometimes you recognize the shape. forepaw tries different ways to figure out what an element is called, from most to least reliable — and remembers which trick worked, so you know whether to trust the label.

Each backend resolves the accessible name through a fallback chain whose priority order mirrors the W3C accessible name computation, and tags the result with a normalized `NameSource` variant: `title`, `description`, `title_ui_element`, `child_label`, `help_text`, `placeholder`, `icon_class`, or `role_description`. The variants are normalized across platforms — `AXTitle` (macOS), UIA `CurrentName` (Windows), and atspi `Name` (Linux) all map to `title`. `name_source` is `Some` iff `name` is `Some`.

For web content the browser has already run the W3C computation and placed the result in the platform's title attribute, so this chain mostly fires for native apps and sparse/incomplete trees. The fallback steps (description, child scan, icon class, role description) are forepaw's own heuristics for the gaps the platform didn't fill.

This separation matters for audits: author-provided names (`title`, `title_ui_element`, `child_label`) are authoritative, while `icon_class` (heuristic), `role_description` (generic fallback), and `placeholder` (not a real label) are low-confidence — the kind of name an accessibility rule should flag.

#### macOS

The full chain, in priority order:

1. **AXTitle** → `title`.
2. **AXDescription** → `description`.
3. **AXTitleUIElement** → `title_ui_element`. A reference to a separate label element. Read that element's `AXValue` or `AXTitle`. This is the only step that makes individual IPC calls (2 calls max), because the referenced element's attributes aren't in our batch.
4. **First child scan** → `child_label`. Check pre-built child `ElementNode` objects for the first `AXStaticText` child's value or `AXImage` child's computed name. No IPC needed: children are built before the parent's name is computed.
5. **AXHelp** → `help_text`. Descriptive help text.
6. **AXPlaceholderValue** → `placeholder`. Text field placeholder.
7. **AXDOMClassList** → `icon_class`. CSS class list, parsed by `IconClassParser` to extract icon names from Lucide, Tabler, FontAwesome, Material, Heroicons, Phosphor, Bootstrap, Feather, Ionicons, Octicons, Codicons prefixes.
8. **AXRoleDescription** → `role_description`. Only when it's more specific than the generic role description. A set of ~35 generic descriptions is filtered out.

The children-first build order is critical: the tree builder recurses into children, builds their `ElementNode` objects (triggering their own name resolution), then computes the parent's name using those already-built children. Step 4 reads from in-memory objects instead of making IPC calls per child.

#### Windows

1. **CurrentName** → `title`.
2. **CurrentHelpText** → `help_text`.
3. **First child scan** → `child_label` — first `StaticText` child with a name.

UIA's tree walk produces flat named elements more reliably than AX, so the chain is shorter.

#### Linux

1. **Name** → `title`.
2. **Description** → `description`.
3. **HelpText** → `help_text`.
4. **First child scan** → `child_label` — first `StaticText` child with a name.

AT-SPI2 exposes `Name` as a D-Bus property (via `org.freedesktop.DBus.Properties.Get` on `org.a11y.atspi.Accessible`).

## Text attributes

> **The raccoon version:** Sometimes it's not enough to know there's writing on the box — you want to know if it's scrawled in marker, printed in bold, or glowing red. forepaw reads the typography and color of text elements so consumers can inspect styling (or check contrast) without re-deriving it from pixels.

`DesktopProvider::get_text_attributes(app, reference)` returns per-run font, color, and decoration info for a text element, as a platform-agnostic `TextAttrsResult` (`core::text_attrs.rs`). Text with mixed formatting is split into `TextAttrsRun` entries, each carrying its own `TextAttributes` (font family/name/size, foreground and background color as `#RRGGBB[AA]`, strikethrough/underline and their colors, superscript, shadow, natural language) over a character range.

On macOS this is parsed from `AXAttributedStringForRange` (a parameterized attribute on `AXStaticText`/`AXTextArea`). `CGColor`→hex and other CoreFoundation conversions live in `darwin/cf_convert.rs`. Windows (`UIA TextPattern.ForegroundColor`/`BackgroundColor`) and Linux (AT-SPI2 `Text` interface `TEXT_ATTR_*`) currently return `None` — to be implemented. The method is library-only: there is no CLI subcommand for it, since the text-attribute values aren't useful as standalone CLI output.

## Typed role enum

> **The raccoon version:** Before forepaw went cross-platform, every UI element was described in the language of whatever city it lived in — macOS called everything "AXButton" and "AXTextField". That's like describing a pizza box, a milk carton, and a tin can each by their barcode — useless if you're a raccoon who just wants to know what you can eat. forepaw has a proper field guide now: button, textfield, checkbox, slider. Same raccoon, better vocabulary.

The `Role` enum (`crates/forepaw/src/core/role.rs`) defines all known UI roles as typed variants. 57 variants covering all three platforms. Each platform backend maps its native role type to the enum:

| Platform | Native role type | Mapping function | Unknown fallback |
|----------|-----------------|-----------------|-----------------|
| macOS | AX role string (`"AXButton"`) | `ax_role_to_role()` | `Role::Unknown` |
| Windows | UIA ControlType ID (i32, e.g. `50000`) | `control_type_to_role()` | `Role::Unknown` |
| Linux | AT-SPI2 role number (u32, e.g. `23`) | `atspi_role_to_role()` (generated) | `Role::Unknown` |

Key methods on `Role`:
- `is_interactive()` — whether the element gets a `@eN` ref
- `short_name()` — human-readable name (`"button"`, `"textfield"`)
- `annotation_category()` — color category for annotated screenshots
- `to_lowercase()` — used in tree rendering output

The `native_role` field on `ElementData` preserves the raw platform string (e.g. `"AXButton"`, `"UIA 50000"`, `"ATSPI 23"`) for debugging. Shown in verbose mode.

The Linux role mapping is generated from the upstream GNOME header (`res/atspi-constants.h`) via `res/generate_atspi_roles.sh` (an `awk` script that parses the C enum). CI checks the generated output matches the checked-in file. Unrecognized roles map to `Role::Unknown` — no compile errors, no runtime panics.

## ElementData and ElementNode

> **The raccoon version:** Like keeping a raccoon's ID card separate from its family tree. The card says "medium-sized, partial left ear, scar on nose." The tree says "Mom, Dad, three siblings, lives behind the Quiznos." Different info, different use cases.

`ElementData` holds flat element properties (role, name, value, bounds, reference, state fields, identifiers, signatures). The `name_source` field records which resolution step produced `name` (see above) — verbose-only in text output, always present in JSON when `name` is. `ElementNode` wraps it with a `children: Vec<ElementNode>` for tree structure.

The split means consumers that don't need tree structure (audit rules, diffing, inspector UIs) can work with `Vec<ElementData>` instead of recursive `ElementNode` traversal. `ElementData` implements `Serialize` directly (no recursion). `ElementNode` serializes with `children` as a recursive serde field.

## UID and signatures

> **The raccoon version:** A fingerprint for each element. The uid is "this is the 3rd thing I touched today" — changes every time you look. The signature is "this thing has a button labeled OK with identifier confirm_btn" — same across days. Helps you recognize the same dumpster lid even when you come back tomorrow.

Every element gets two identifiers during tree assignment:

- **uid**: Sequential depth-first counter. Unique within a single snapshot. Resets on every snapshot call.
- **signature**: FNV-1a 64-bit hash of `(role, name?, identifier?, native_role?)`. Same content across snapshots → same hash. Enables cross-snapshot element matching without storing full text.
- **signature_bounds**: Same as signature but includes rounded element bounds — disambiguates content-identical elements at different positions.

The FNV-1a implementation is self-contained in `crates/forepaw/src/core/signature.rs` (no external dependency). Length-prefixed field serialization prevents boundary ambiguity: `name="AB"` + no identifier hashes differently from `name="A"` + `identifier="B"`. Each field feeds as `length_as_u64_le8 || field_bytes`, with `None` contributing length=0 and no bytes.

Known-answer test vectors from the reference `fnv` crate validate correctness.

## Element state

> **The raccoon version:** A button isn't just a button. Sometimes it's grayed out and can't be pressed. Sometimes it's already highlighted, like the lid you just pried open. forepaw tells you these things.

Elements carry four state fields populated by each platform backend:

| Field | macOS | Windows | Linux |
|-------|-------|---------|-------|
| `enabled` | `AXEnabled` | `CurrentIsEnabled` | AT-SPI2 StateSet `ENABLED` |
| `focused` | `AXFocused` | `CurrentHasKeyboardFocus` | AT-SPI2 StateSet `FOCUSED` |
| `selected` | `AXSelected` | `CurrentIsSelected` | AT-SPI2 StateSet `SELECTED` |
| `description` | `AXDescription` | `CurrentHelpText` | Description D-Bus property |

State fields are `Option<bool>` — `None` means the platform doesn't provide this property. Non-`None` values appear in tree rendering as tags: `disabled`, `focused`, `selected`. Description appears in verbose mode only.

On macOS, these four fields come from the same batched `AXUIElementCopyMultipleAttributeValues` call as the rest of the attribute set — no additional IPC cost.

Note: `AXFocused` returns `true` for both "this element can receive focus" and "this element currently has focus" on macOS (a known W3C Core-AAM spec issue). The actual focused element is available via `AXFocusedUIElement` on the application element, which is a separate IPC call not made during tree walking.

## Tree pruning

> **The raccoon version:** A smart raccoon doesn't dig through the recycling bin when the pizza box is right on top. forepaw skips parts of the tree that can't possibly contain anything useful — stuff that's off-screen, invisible, or in the menu bar. Less digging, faster meals.

The `PruningOptions` struct (`crates/forepaw/src/core/tree_pruning.rs`) controls three independent mechanisms, applied during the tree walk:

**Offscreen pruning** (default-on, depth > 2): Skips subtrees whose bounds are entirely outside the window rect. The intersection test compares element bounds (screen-absolute) against window bounds. Elements without bounds or with zero size are not pruned (they're structural containers). This is what makes Apple Music usable — 800+ invisible play history rows at negative Y coordinates get skipped.

**Zero-size pruning** (interactive mode only, depth > 1): Skips subtrees rooted at 0x0 elements — collapsed menus, hidden panels, offscreen content with no spatial extent.

**Menu bar pruning** (interactive mode only): Skips the `AXMenuBar` subtree entirely. Menu bars contain 200-300 elements that agents rarely need.

All three are bypass-able: `--offscreen`, `--zero-size`, `--menu` include them back.

The pruning module is shared across all three platforms. Windows's `ControlViewWalker` and Linux's unrestricted `GetChildren` benefit from the same pruning logic. Depth-gating rules (depth > 1 for zero-size, depth > 2 for offscreen) prevent pruning structural containers at the top of the tree.

## Electron app detection

> **The raccoon version:** Some trash cans have a second, hidden compartment that only opens if you press a secret latch. Electron apps are like that — they have a full accessibility tree inside, but they don't build it unless you ask nicely. forepaw presses the latch.

macOS only. forepaw detects Electron apps by checking for `Contents/Frameworks/Electron Framework.framework` in the app bundle. When detected:

1. Sets `AXManualAccessibility` on the app element — Chromium's official API for third-party assistive technology.
2. Polls for up to 3 seconds until the `AXWebArea` has interactive children. The polling uses the batched attribute fetch path (`AXUIElementCopyMultipleAttributeValues`) rather than individual attribute calls — individual calls return errors on partially-built Electron trees, causing the poll to always time out.
3. Uses depth 25 instead of 15 (Electron's DOM nesting creates 13+ levels of groups).

`AXManualAccessibility` is idempotent and is also set during ref resolution (action dispatch), so clicking works without a preceding snapshot.

Electron apps using icon libraries (Lucide, Tabler, FontAwesome, etc.) get automatic icon names via `IconClassParser`, which strips known CSS class prefixes. An unnamed button with class `lucide-settings` becomes `button "settings"`.

### CEF vs Electron

CEF (Chromium Embedded Framework) apps like Spotify and Steam use the same Chromium engine as Electron but don't respond to `AXManualAccessibility`. The CEF accessibility bridge requires each embedding application to implement it — unlike Electron, which provides a universal activation mechanism. CEF apps are detected by `Chromium Embedded Framework.framework` in the bundle but are not treated as Electron apps. They are OCR-only.

## Ref system

> **The raccoon version:** Raccoons remember routes by sequence — third tree, second fence, first dumpster. forepaw numbers every interactive element in the order it finds them. `@e3` means "the 3rd interactive thing I touched on my walk through the tree."

Refs are assigned by `RefAssigner` (`crates/forepaw/src/core/ref_assigner.rs`), which walks the tree depth-first and assigns sequential numbers to interactive elements. The interactive role set: button, text field, text area, checkbox, radio button, slider, combo box, popup button, menu button, link, menu item, tab, switch, incrementor, color well, tree item, cell, dock item.

The `interactive_only` flag controls whether non-interactive elements get refs. In snapshot mode, only interactive elements get `@eN` labels. The `--interactive` flag includes all elements if needed.

During tree construction, native element handles are collected alongside ref positions and cached on the provider, so ref resolution is an O(1) map lookup instead of a tree re-walk. The cache is replaced wholesale on each `snapshot`. macOS (`DarwinProvider`) retains `AXUIElementRef` (manual `CFRetain`/`CFRelease` on the map's `Drop`); Windows (`WindowsProvider`) holds owned `IUIAutomationElement` (RAII `AddRef`/`Release`). Both share the same numbering walk in `core::ref_cache` (`HandleNode<H>` + `flatten_handles`). Linux has no cache yet.

### Ref resolution across invocations

Refs are just positional numbers. On macOS, resolving a ref (e.g. `click @e10 --app Finder`) returns the `AXUIElement` handle retained during the most recent `snapshot` on the same provider instance — an O(1) lookup. This is what makes a single in-process pass over N elements cheap: N full tree walks collapse to N lookups. When no handle is cached, it falls back to a full re-walk that mirrors `RefAssigner`'s depth-first counter over interactive elements. That fallback fetches only role + children per node (no batched attribute fetching), so it is slower per node than the snapshot walk — the cost grows with tree size. The CLI builds a fresh provider per invocation, so its one-shot commands always fall back to the re-walk; the cache benefits long-lived in-process consumers.

A ref is only valid against the snapshot that produced it. If the UI changed between snapshot and action (menu opened, dialog appeared), the ref is stale and may point to a different element — "resolve targets the latest snapshot on this provider" is the contract. The fallback re-walk uses the same depth as snapshot (15 for native, 25 for Electron); using a non-default `--depth` on snapshot makes refs inconsistent with action commands that hit the fallback.

## Action dispatch

> **The raccoon version:** Raccoons don't just observe — they manipulate. Twist lids, pull latches, press buttons. forepaw simulates keyboard and mouse input through macOS's CGEvent system, which is like having invisible raccoon paws that the OS can't distinguish from real human input.

Action commands are fully implemented on macOS and Windows (click/type/hover by ref and by coordinates, `press`, `keyboard-type`, scroll, drag, OCR actions, app activation). Linux stubs all action methods. Each stub returns a clear `ActionFailed` error rather than crashing.

### Click

For most roles, tries `AXPress` first (the accessibility action). For `AXLink` elements, right-clicks, and double-clicks, uses mouse click directly — browsers don't navigate on `AXPress` for web content links, and `AXPress` can't express right-click or double-click. Falls back to CGEvent mouse click at the element's center coordinates.

The click path: move cursor to target via `mouseMoved` event (50ms settle time), then `leftMouseDown` + `leftMouseUp`. The pre-move ensures the click routes to the correct window under the cursor. Double-click uses `mouseEventClickState` to signal the click sequence number. Right-click uses `rightMouseDown` + `rightMouseUp`.

On Windows, the same two-tier model applies: `click @ref` tries `InvokePattern.Invoke()` first, then falls back to `SetCursorPos` + `SendInput` mouse click at the element's center. Right-click and double-click skip Invoke (it takes no arguments, so it can't convey button or count) and go straight to the mouse path.

### App activation

Before any mouse click or keystroke targeting an app, `NSRunningApplication.activate()` is called with a 300ms delay. CGEvent posts to whatever window is under the cursor, so the target app must be frontmost. Without activation, clicks go to the wrong window. This 300ms delay is the largest fixed cost in action dispatch.

On Windows, `SetForegroundWindow` brings the target window forward with the same 300ms settle. `SendInput` delivers to the foreground window's focused control, so activation is equally necessary there.

### Type

Tries `AXUIElementSetAttributeValue` on the element's value first. Falls back to focusing the element via `AXRaise` + `AXFocused` and simulating keystrokes.

On Windows, `type @ref` tries `ValuePattern.SetValue()` first, then falls back to `SetFocus()` + `keyboard-type`.

### Keyboard input (Windows)

`keyboard-type` and `press` use Win32 `SendInput`, the Windows parallel to macOS's CGEvent. Text is sent as Unicode key events so any character types regardless of keyboard layout.

**Modifier mapping:** `Modifier::Command` maps to `VK_CONTROL` on Windows, not the Windows key, so `cmd+s` resolves to Ctrl+S — matching macOS Cmd+S semantically and keeping agent scripts portable across platforms. `Control` is also Ctrl, `Option` is Alt (`VK_MENU`), `Shift` is `VK_SHIFT`.

### Mouse input (Windows)

Coordinate-based **clicks** position the cursor with `SetCursorPos` (physical pixels, multi-monitor-correct) and post button down/up events with `SendInput`. **Hover** uses `MOUSEEVENTF_ABSOLUTE` moves instead (see [Hover](#hover)). Multi-click relies on the OS's down/up-timing detection (`MOUSEINPUT` has no click-count field). Ref-based click/hover tries the UIA pattern first (`InvokePattern.Invoke` for click, `ValuePattern.SetValue` for type), then falls back to the mouse/keyboard path. Region clicks use geometric center (saliency not yet wired).

### Hover

`moveMouse` posts a single `mouseMoved` event with 50ms settle time. `smoothMoveMouse` interpolates 20 intermediate `mouseMoved` events over 150ms for apps that track `mouseEnter`/`mouseLeave` (e.g., Orion's auto-hiding sidebar). Without smooth movement, teleporting the cursor doesn't trigger tracking area events.

**Hover on Windows**: `SetCursorPos` alone doesn't trigger hover effects — a Win10 build-16299+ regression stops its synthesized `WM_MOUSEMOVE` reaching many apps (Start menu, Edge). Relative `SendInput` moves overshoot when mouse-speed/acceleration is non-default. So hover uses `MOUSEEVENTF_ABSOLUTE` moves: normalized to 0..65535 over the virtual desktop, they hit the exact pixel (no acceleration) *and* inject a real, honored event. `hover_move` interpolates ~15 absolute moves to the target, then dwells. `--smooth` is a no-op for Windows hover (it always interpolates); macOS still honors the flag.

### Scroll

Scroll events use `CGEvent(scrollWheelEvent2Source:...)` with line units. The mouse is moved to the target point first. Scroll amount is in "ticks" (lines).

**Boundary detection**: After scrolling, forepaw captures a pixel fingerprint of the window — a 20px horizontal strip from the vertical center, excluding the rightmost 30px to avoid transient scrollbar overlays — using `CGWindowListCreateImage`. If the fingerprint matches the pre-scroll capture, the scroll hit a boundary and the message says so.

**Boundary detection on Windows**: Same fingerprint approach as macOS, using GDI instead of CoreGraphics. `BitBlt` captures a 20px horizontal strip from the window's vertical center (screen DC, excluding rightmost 30px for scrollbars) into a pixel buffer. Compare before/after the wheel event; equal means at boundary. Implemented in `capture_strip_fingerprint` (`windows/screenshot.rs`).

**Scroll on Windows**: `SendInput` `MOUSEEVENTF_WHEEL`/`HWHEEL`. Direction maps to sign — the delta is `amount * WHEEL_DELTA` as two's-complement in the `u32 mouseData` field. Target resolution: `--at` (validated, window-relative) → `--ref` (resolve center via cache or rewalk) → default window center. Cursor moves to the target first (wheel goes to the window under the cursor).

### Drag

Move to start, post `mouseDown`, interpolate through path segments with `mouseDragged` events, then post `mouseUp`. Steps per segment (default 30) and total duration (default 0.3s) control smoothness. Modifier keys and pressure are applied to every event in the drag.

**Drag on Windows**: Same two-tier approach as macOS. `SetCursorPos` for the exact start point, then relative `SendInput` `MOUSEEVENTF_MOVE` deltas for the interpolated body (real injected input, not synthesized `WM_MOUSEMOVE`), then `SetCursorPos` to snap to the exact endpoint before button-up. The relative-move form matters: an earlier `SetCursorPos`-only version moved the cursor but didn't draw in Paint — modern apps ignore synthesized moves for drag operations. Modifiers held for the entire drag. (Relative moves scale with mouse speed; endpoints are `SetCursorPos`-pinned regardless. Hover moved off relative moves for this reason — see [Hover](#hover).)

### OCR actions

`ocr-click` finds text via OCR and clicks its center; `hover "text"` does the same for hover; `wait` polls OCR until text appears. Each composes the OCR engine with the click/hover primitives. On Windows, OCR coordinates are physical pixels relative to the captured window's top-left (`GetWindowRect` origin), translated to screen-absolute via `to_screen_point` — the same conversion coordinate clicks use.

## Coordinates

> **The raccoon version:** Different dumpsters have different layouts. Inside a can, everything is relative to that can's rim. But the city map uses absolute positions. forepaw handles both — the snapshot shows you inside-the-can positions, actions use the absolute city coordinates to actually hit things.

Each platform defines its own coordinate space. forepaw normalizes for display and keeps absolute for actions.

| Platform | Display coordinates | Action coordinates | Source |
|----------|-------------------|-------------------|--------|
| macOS | Window-relative | Screen-absolute | AX returns screen-absolute; `enrich()` derives window-relative |
| Windows | Window-relative | Screen-absolute | UIA returns physical pixels; `enrich()` derives window-relative |
| Linux | Window-relative | Screen-absolute | AT-SPI2 returns absolute pixel coordinates; `enrich()` derives window-relative |

All coordinates in snapshot output are window-relative: `(0,0)` is the window's top-left corner. Elements store screen-absolute `bounds` internally (as reported by the platform API); `ElementTree::enrich()` runs once after the tree is built to populate each node's `bounds_window` = `bounds` minus the window origin. The text renderer prints `bounds_window`; JSON emits both `bounds` (screen-absolute) and `bounds_window` (window-relative), so consumers never have to re-derive the subtraction themselves. `filter_tree` re-runs `enrich()` because pruning rebuilds nodes.

### macOS

The absolute-to-relative conversion happens at the provider layer. The CLI passes window-relative coordinates through; the provider adds the window origin before CGEvent calls.

**Coordinate validation**: `CoordinateValidation` checks `0..width, 0..height` against window bounds. Applied to `click`, `hover`, `scroll --at`, and `drag` when `--app` is specified. Rejects out-of-bounds coordinates with an error (not a clamp — a misplaced click could be destructive).

**Exception**: `hover` without `--app` treats coordinates as screen-absolute (for global positioning like Spotlight/Raycast).

### Windows

Physical pixel coordinates throughout. `GetWindowRect` returns physical pixels. UIA's `CurrentBoundingRectangle` returns physical pixels. GDI `BitBlt` captures at physical pixel resolution. DPI awareness is set to `PER_MONITOR_AWARE_V2` at startup — all coordinate sources use the same pixel space.

### Linux

Screen-absolute pixel coordinates from AT-SPI2's `GetExtents` method. Same DPI considerations as Windows (physical pixels throughout).

### OCR coordinates

On macOS, the window screenshot (`screencapture -l`) has `(0,0)` at the window's top-left in image-pixel space. After Vision's bottom-left normalization is undone, coordinates are divided by `backingScaleFactor` (typically 2.0) to get logical window-relative coordinates. No window origin adjustment needed — the image is already window-scoped.

On Windows, the OCR screenshot is a full-screen capture (or window-specific via `PrintWindow`). When using `PrintWindow`, coordinates are window-relative (the image is cropped to the window). When using desktop `BitBlt`, coordinates are screen-absolute.

## Displays

> **The raccoon version:** Before raiding, a raccoon scopes the whole alley — how many dumpsters, how big, which one's under the bright light. `list-displays` does that.

`DesktopProvider::displays()` returns a `DisplayInfo` per physical monitor: logical bounds, backing scale factor, name, color space, refresh rate, and primary/builtin flags. Consumers that map logical coordinates to pixels (screenshot sampling, OCR image-to-logical conversion) read the scale from the specific display a window sits on, not a global assumption.

`logical_bounds` semantic differs by platform, matching the platform's coordinate space above: macOS returns logical points (CGDisplayBounds); Windows returns physical pixels (GetMonitorInfoW under `PER_MONITOR_AWARE_V2`). Windows `logical_bounds` thus don't shrink when the user raises the scale factor — the physical framebuffer size is constant; the scale factor is a separate multiplier. Consumers wanting logical dimensions on Windows divide `logical_bounds` by `scale_factor`.

| Field | macOS | Windows | Linux |
|-------|-------|---------|-------|
| `id` | CGDirectDisplayID | HMONITOR (cast) | — (stub) |
| `name` | NSScreen.localizedName | GDI device name (`\\.\DISPLAY1`) | — |
| `scale_factor` | CGDisplayMode pixel/logical ratio | GetDpiForMonitor / 96 | — |
| `color_space` | NSColorSpace.localizedName | ICC profile filename stem (GetICMProfileW) | — |
| `refresh_rate_hz` | NSScreen.maximumFramesPerSecond | EnumDisplaySettingsW dmDisplayFrequency | — |
| `is_hdr` | maximumExtendedDynamicRangeColorComponentValue > 1.0 | None (needs QueryDisplayConfig) | — |
| `is_builtin` | CGDisplayIsBuiltin | None (needs EDID via SetupAPI) | — |

macOS reads id/bounds/scale/primary/builtin from thread-safe CoreGraphics; name/color/refresh/hdr come from `NSScreen`, which is main-thread-only, so those fields are best-effort `None` when called off the main thread.

## Window resolution

> **The raccoon version:** You're a raccoon looking at three dumpsters behind a restaurant. Which one has the good stuff? forepaw picks the right window — by ID, by title, or just the biggest one in sight.

### macOS

`findWindow` resolves which window to target:

1. If `window` starts with `w-`, match by `CGWindowID`.
2. If `window` is provided, case-insensitive substring match against window titles.
3. Otherwise, pick the largest non-phantom window (>= 10px wide) preferring titled windows.

The titled-window preference avoids Finder's full-screen desktop window (larger but untitled) and similar invisible windows. Window info comes from `CGWindowListCopyWindowInfo(.optionOnScreenOnly, ...)`.

Some apps (Steam) render their UI in a helper process with `accessory` activation policy. The main process (shown in `list-apps`) has only phantom windows. When `findWindow` finds no usable windows for the main PID, it falls back to searching `CGWindowList` for onscreen windows owned by processes with the same bundle ID prefix (e.g., `com.valvesoftware.steam` matches `com.valvesoftware.steam.helper`). This enables screenshots, OCR, and coordinate-based actions on Steam and similar multi-process apps.

### Windows

`EnumWindows` enumerates all top-level windows. For each HWND, `GetWindowTextW` reads the title and `GetWindowRect` reads bounds. The `--window-id` parameter matches the HWND value displayed as `w-{hwnd}` in `list-windows`.

UWP apps (Calculator, Settings, Photos) run inside `ApplicationFrameHost.exe`. Multiple UWP apps can share the same host process. The app enumeration detects this and emits one `AppInfo` per titled ApplicationFrameHost window instead of one per PID — so `list-apps` shows "Calculator", "Settings", etc. as separate apps.

### Linux

`list-apps` queries the AT-SPI2 registry's root object for registered applications. Each application's children are scanned for `FRAME` (Qt) or `WINDOW` (GTK) roles. Window info includes the D-Bus object path, title, and bounds from `GetExtents`. The `--window-id` parameter matches the D-Bus object path.

## Region targeting (saliency detection)

> **The raccoon version:** A raccoon can't describe *exactly* where the shiny thing is in the garbage bag, but it knows which *part* of the bag it's in. It reaches into that area and grabs the most interesting thing it touches. Region click works the same way — point at an area, and forepaw finds the shiniest thing in it.

`click x,y,w,h` and `hover x,y,w,h` target a rough area instead of precise coordinates. macOS only.

`SaliencyDetector` captures a screenshot, crops the specified region, and finds the centroid of high-saturation pixels. The pipeline:

1. Capture window screenshot via `screencapture -l`
2. Crop to the specified region (in Retina pixel coordinates)
3. Render pixels into an RGBA buffer via `CGContext`
4. Compute HSL saturation per pixel; also track brightness deviation from median as fallback for desaturated icons (white on dark)
5. Weighted centroid of pixels above saturation threshold (0.25) or brightness deviation threshold (0.3)
6. Convert centroid from crop-pixel coordinates to window-relative logical coordinates
7. Click or hover at the centroid

Why saturation? UI buttons are colored (green play, blue links, red close, orange warnings). Backgrounds are desaturated (gray, black, white). The most saturated pixels in a small region are almost always the target button.

The agent's job becomes "draw a rough box around the target" (which LLMs can do from screenshots) instead of "predict exact pixel coordinates" (which LLMs cannot reliably do).

## OCR

> **The raccoon version:** Sometimes the dumpster is sealed and you can't feel inside — you have to *look* at it. OCR takes a picture of the window and reads the text, like a raccoon squinting at a label through the plastic.

### macOS: Vision framework

1. Capture full-resolution PNG via `screencapture -l <windowID>` (2x Retina)
2. Run `VNRecognizeTextRequest` (`.accurate`, no language correction)
3. Convert Vision's normalized bottom-left coordinates to window-relative logical pixels
4. Optionally save an agent-friendly display copy (WebP/JPEG, 1x)

When `--find` is specified, `findPrecise` gets word-level bounding boxes for the matched substring via `VNRecognizedText.boundingBox(for:)` rather than using the full observation's bounding box. This gives precise click coordinates for individual words inside a paragraph. Falls back to block-level filtering if precise matching finds nothing.

OCR settings:
- Recognition level: `.accurate` (not `.fast`) for reliability
- Language correction: disabled — preserves usernames, IDs, and technical text that autocorrect would mangle

### Windows: Windows.Media.Ocr

The OCR module captures a screenshot, upscales it 3× using Lanczos3 filtering via the `image` crate, then passes it to `Windows.Media.Ocr.OcrEngine` via WinRT.

The 3× scale eliminates character confusions like 0 vs Ø (empirically determined at 1×/2×/3×/4× on VM). The 1/l confusion is unresolvable at any scale. The upscaling adds latency: ~200ms native → ~600ms at 3× on ARM64.

WinRT async is bridged to blocking calls via Win32 events (`CreateEventW` + `WaitForSingleObject`) rather than Rust async runtimes — the `DesktopProvider` trait is synchronous.

### Linux

Not implemented. Planned: `tesseract` CLI integration.

## Screenshot processing

> **The raccoon version:** A picture of a dumpster is useless if it's too dark to see the labels. forepaw processes screenshots — crops, scales, converts format — so agents get a clean, compact image they can actually read.

### macOS

All screenshots start as full-resolution PNG via `screencapture -l <windowID>`. Post-processing:

1. **Scale** (1x mode): `sips --resampleWidth` halves the Retina image to logical pixels.
2. **Format**: WebP via `cwebp` (external binary, must be installed), JPEG via `sips`, or kept as PNG.
3. **Crop**: `--ref @eN` resolves element bounds, `--region x,y,w,h` uses window-relative coordinates. Both add padding (default 20px). Cropping uses CoreGraphics `CGImage.cropping(to:)`.

Default output: best available format (WebP if `cwebp` installed, else JPEG), logical (1x) scale, cursor visible. Agent-optimized at ~85-150KB per window.

### Windows

Two-tier capture approach:

1. **Try `PrintWindow` with `PW_RENDERFULLCONTENT` (value 2)** — captures the window's own content directly, even when occluded by other windows. Works with DWM-composed windows (UWP, Chromium, WinUI 3).
2. **Fall back to desktop DC `BitBlt`** — captures from the screen DC if `PrintWindow` fails.

The `PW_RENDERFULLCONTENT` flag is undocumented but stable since Windows 8 (same technique Chromium uses internally). Images are saved as PNG via the `image` crate. RGBA ↔ BGRA conversion happens in every capture path: GDI and WinRT use BGRA pixel format, the `image` crate expects RGBA.

### Linux

Not implemented. Planned: `spectacle` (KDE) or `magick import` (X11) CLI integration.

### Capture density request (`CaptureScale`)

`ScreenshotOptions.scale` is a [`CaptureScale`] enum, not an integer, because the consumer intent ("give me native pixels" / "give me logical pixels") is a closed dichotomy the platform resolves per-display — not an upscale factor. It pairs with the truth-reporting fields below: the consumer requests `Native`, forepaw resolves "native" per-display via `display_for_bounds`, and the consumer confirms the actual ratio via the reported `pixels_per_bound_unit` before trusting pixel math.

- [`CaptureScale::Native`] — best available per platform: macOS backing pixels (2× on Retina), Windows physical pixels. The reported `pixels_per_bound_unit` tells the consumer the actual ratio.
- [`CaptureScale::Logical`] — point/logical space, downsampled from backing on `HiDPI` displays (macOS `sips --resampleWidth`; Windows in-memory Lanczos3 resize by the display's `scale_factor`).

### Reported scale and dimensions

`ScreenshotResult` carries `pixels_per_bound_unit` and `pixel_dimensions` so consumers know exactly what the returned image is without re-querying the display:

- **`pixels_per_bound_unit`** — pixels per snapshot-bound-unit of the returned image (what it *is*, not what was *requested*). Multiply a bound-unit delta (`element_bounds − window_origin`) by it to get image pixel coordinates. Equals `DisplayInfo.scale_factor` only where snapshot bounds are logical units: macOS, where bounds are points (so this is 2.0 on Retina under `CaptureScale::Native`, 1.0 under `Logical`). On Windows, snapshot bounds are **physical pixels** (the process runs `PER_MACHINE_AWARE_V2`), so a `Native` capture reports 1.0 here — the bounds you received are already the image's pixels — while `Logical` reports 1/display-scale. The display's DPI ratio always lives on `DisplayInfo`; this field is the per-image ratio that makes `(bounds_delta) × this` correct.
- **`pixel_dimensions`** — actual pixel width/height of the returned image after any resampling, computed from the logical extent (window size, or crop rect for crops) at the reported scale. Lets consumers validate their scale math without decoding the bytes.

The scale is derived from the display the captured window sits on (via majority-overlap lookup against `displays()`), not the main screen — so a window on a non-primary display reports its own scale. The no-`--app` (full-screen) path falls back to the main display's scale: an approximation that is exact on single-display setups but imprecise on multi-display, where a targetless `screencapture` captures the composite virtual desktop spanning displays of potentially different scales (a single `scale_factor` is ill-defined for that composite).

Consumers wanting native-resolution sampling (e.g. contrast sampling at backing resolution rather than a hardcoded 1x) should request `CaptureScale::Native` (CLI `--scale native`) and read `pixels_per_bound_unit` to confirm what they received.

## Annotated screenshots

> **The raccoon version:** Sometimes you need a map of the dumpster. Annotated screenshots overlay numbered labels on every interactive element, like someone put little flags on every lid, handle, and latch so you can say "open flag 3" without fumbling around.

The annotation pipeline is split across crate boundaries:

1. **`crates/forepaw/src/core/annotation.rs`** — walks the element tree, collects annotation structs for interactive elements with bounds, converts to window-relative coordinates, filters off-screen elements.
2. **`crates/forepaw/src/platform/darwin/annotation.rs`** — draws on the image via CoreGraphics. Three styles: `badges` (numbered pills), `labeled` (bounding boxes with role+name), `spotlight` (dims non-interactive areas). Color-coded by category: green=buttons, yellow=text fields, blue=selection controls, purple=navigation.
3. The core `Annotation` module formats the text legend mapping display numbers to refs.

Annotations are rendered on the full window image, then cropped if `--ref` or `--region` is specified. Windows and Linux backends don't support annotation rendering yet.

## Snapshot diffing

> **The raccoon version:** A raccoon returns to the same dumpster each night and immediately notices what changed — new bags, missing boxes. Snapshot diffing does the same: compare before and after to see exactly what moved.

`SnapshotDiffer` compares two rendered snapshot texts using LCS (longest common subsequence) diff. Refs are stripped before comparison (`@e5` removed from lines) so positional ref shifts from added/removed elements don't produce false changes. The output shows the new refs.

Snapshots are cached per-app in temp files (`/tmp/forepaw-snapshot-<app>.txt`). No manual baseline management — `snapshot` caches automatically, `snapshot --diff` loads the previous and compares.

The diffing is text-based (rendered output, not tree structure). This is cross-platform: any backend's rendered tree can be diffed against any other.

## Timing diagnostics

`--timing` on snapshot outputs a per-subtree breakdown to stderr. `SnapshotTiming.report()` adaptively expands subtrees holding >10% of total nodes and collapses single-child wrapper chains (common in Electron's deep DOM nesting). Output goes to stderr so it doesn't pollute the snapshot output that gets cached or diffed.

## Logging

> **The raccoon version:** When things go wrong, a raccoon doesn't just sit there — it sniffs around, checks the ground, listens for footsteps. Forepaw's logging is the same: you can set `FOREPAW_LOG=snapshot=debug` to hear what the tree walker is doing, without the noise from every other module.

Zero-dependency logging via `FOREPAW_LOG` env var (falls back to `RUST_LOG`, then defaults to `warn`):

- `FOREPAW_LOG=debug` — global debug level
- `FOREPAW_LOG=snapshot=debug,app=info` — per-module overrides
- `FOREPAW_LOG=warn,snapshot=debug` — global level + override

Module names are matched against the module path with `forepaw::` and platform prefix stripped, so `snapshot=debug` works across `forepaw::platform::darwin::snapshot` and `forepaw::platform::windows::snapshot` — same module name, different platform paths.

Five levels: Error (1), Warn (2), Info (3), Debug (4), Trace (5). The `enabled()` check uses integer comparison.

Macro-based invocation: `forepaw::debug!("tree walk took {:.1}ms", elapsed);`

## Permissions

> **The raccoon version:** Some dumpsters are locked. The city says you need a permit to open certain bins. macOS has two locks: one for feeling around inside (accessibility), one for looking at labels (screen recording). Windows and Linux just let you in.

| Platform | Observation | Actions | Screenshot / OCR |
|----------|-----------|---------|-----------------|
| macOS | AX permission | AX permission | Screen Recording permission |
| Windows | Always available | Stubbed | Always available |
| Linux | Always available | Stubbed | Always available |

macOS permissions are per-app (granted to the terminal, not to forepaw itself). After granting, the terminal may need a restart for the permission to take effect.

macOS has an additional validation step: after `CGPreflightScreenCaptureAccess()` returns true, `validate_screen_recording()` checks the actual window list for third-party app windows. The permission prompt can succeed but still redact windows from a new binary for the first few seconds. The window-list check catches this.

## Platform architecture

> **The raccoon version:** Raccoons live everywhere — cities, forests, suburbs. forepaw is designed the same way: the core logic (how to number elements, render trees, parse keys) is habitat-agnostic. Only the paws are specialized — different grips for different dumpsters.

The `DesktopProvider` trait (`crates/forepaw/src/platform/mod.rs`) defines the full platform surface — ~20 methods covering observation (list-apps, list-windows, snapshot, element-at-point) and actions (click, type, press, scroll, drag, hover, wait). All CLI commands call through `&dyn DesktopProvider`. If a method isn't on the trait, it won't compile from command code.

All platform-specific types in the trait use platform-agnostic variants (`Point`, `Rect`, `AppTarget`, `WindowTarget`). Each provider converts internally.

Three backends, each in `crates/forepaw/src/platform/{darwin,windows,linux}/`, selected at link time via cfg attributes. Each platform's code only exists in its own cfg-gated module — `cargo check --target windows` on macOS compiles only the cross-platform trait and core types.

The workspace has two crates. `forepaw` (library, `crates/forepaw/`) contains core types, the `DesktopProvider` trait, and all platform backends. `forepaw-cli` (binary, `crates/forepaw-cli/`) contains CLI argument parsing and command dispatch. The library uses minor-range dependency pins for downstream consumers. The CLI uses exact dep pins for supply chain control.

### Module layout

```
crates/forepaw/src/
├── core/                    # Platform-agnostic types and logic
│   ├── role.rs              # Typed Role enum (57 variants)
│   ├── element_tree.rs      # ElementData, ElementNode, ElementTree
│   ├── ref_assigner.rs      # Depth-first ref + uid + signature assignment
│   ├── tree_renderer.rs     # Text output (default + verbose mode)
│   ├── output_formatter.rs  # JSON/text output dispatch
│   ├── snapshot_diff.rs     # LCS-based line diff
│   ├── snapshot_cache.rs    # Temp-file snapshot caching
│   ├── annotation.rs        # Annotation data structures and legend
│   ├── tree_pruning.rs      # PruningOptions, should_prune
│   ├── signature.rs         # FNV-1a content hashing
│   ├── icon_class_parser.rs # CSS class → icon name
│   ├── text_attrs.rs       # TextAttributes / TextAttrsRun / TextAttrsResult
│   ├── coordinate_validation.rs
│   ├── crop_region.rs       # Rect padding and scale conversion
│   ├── key_combo.rs         # Key combo parsing, ClickOptions, DragOptions
│   ├── cast.rs              # Checked numeric casts for FFI
│   ├── temp.rs              # Collision-resistant temp file paths
│   └── errors.rs            # ForepawError enum
├── platform/
│   ├── mod.rs               # DesktopProvider trait
│   ├── darwin/              # macOS backend
│   ├── windows/             # Windows backend
│   └── linux/               # Linux backend
└── log.rs                   # Zero-dependency structured logging

crates/forepaw-cli/src/
├── main.rs                  # Entry point, cfg-gated provider, command dispatch
├── cli/
│   ├── mod.rs               # GlobalArgs, AppTargetArgs, WindowTargetArgs
│   ├── parse.rs             # Shared parsing utilities
│   ├── observation.rs       # snapshot, screenshot, list-apps, list-windows, ocr, hit-test
│   ├── action.rs            # click, type, keyboard-type, press, hover, drag, scroll, batch, wait, ocr-click
│   └── system.rs            # permissions
└── build.rs                 # Compile-time git SHA embedding
```

### Build-time git SHA

`--version` output includes the commit SHA: `forepaw 0.4.0 (abc1234)`. Embedded at compile time by `build.rs`:

1. Checks `FOREPAW_GIT_SHA` env var first (set by Nix flakes for reproducible builds).
2. Falls back to `git rev-parse --short HEAD`.
3. Falls further to `"unknown"`.
4. Appends `-dirty` when `git status --porcelain` shows any changes.

## Known limitations

### macOS
- **Retina coordinate math**: OCR coordinates assume the primary display's scale factor. Multi-display setups with different scale factors may produce incorrect click positions.
- **Menu timing**: Opening a dropdown menu changes the accessibility tree. A snapshot taken immediately after clicking a menu button may not include menu items.
- **Web content in browsers**: AXPress on links doesn't trigger navigation in some browsers (confirmed in Orion). Mouse click works but requires app activation.
- **Window-specific screenshots**: Uses `screencapture -l <windowID>` which captures the window as-is, including any overlapping windows.
- **Ref depth mismatch**: Using a non-default `--depth` on `snapshot` makes refs inconsistent with action commands.
- **Hover teleportation**: Without `--smooth`, hover teleports the cursor, which doesn't trigger `mouseEnter`/`mouseLeave` tracking areas.
- **Scroll fingerprinting**: Uses `CGWindowListCreateImage` (deprecated in macOS 14). Should migrate to ScreenCaptureKit.
- **VM guest typing**: `keyboard-type` sends wrong characters into VM guests. `press` commands work. VM hypervisors intercept CGEvent keystrokes differently.
- **CEF apps have no AX tree**: Spotify, Steam — OCR only, no `@e` refs.
- **Region click assumes colored targets**: Saliency detection works for colored buttons on desaturated backgrounds. May struggle with monochrome UIs.

### Windows
- **Element state**: `enabled`, `focused`, `selected`, `description`, `identifier`, `native_role` are all `None`. UIA provides all of these — they need wiring into the tree walk.
- **Window state**: `WindowInfo.state` is always `None`. Needs `IsIconic()` (minimized), `IsZoomed()` (maximized), bounds comparison (fullscreen).
- **`is_active`**: Always `false` on `list-apps`/`list-windows`. Needs `GetForegroundWindow()` + `GetWindowThreadProcessId()`.
- **Action commands**: click, type, hover (by ref and coordinates), press, keyboard-type, scroll, drag all implemented. Remaining stubs: `ocr_click`, `ocr_hover`, `wait` (compose OCR + click/hover primitives).
- **OCR latency**: ~600ms at 3× upscale. The 1/l character confusion persists at all scales.
- **`PW_RENDERFULLCONTENT` is undocumented**: If Microsoft changes or removes this flag, per-window capture needs a fallback.
- **No `CacheRequest`**: Each UIA property is an individual COM call. Large trees are slower than macOS's batch approach.

### Linux
- **Element state, window state, `is_active`**: All `None` — same gaps as Windows.
- **Action commands**: All stubbed.
- **Screenshot and OCR**: Not implemented.
- **No batch property fetch**: Each property is an individual D-Bus call. Large trees (1000+ nodes) can take 7+ seconds.
- **KDE Plasma desktop elements**: Hit-testing returns `Role::Unknown` for desktop panel, system tray, and other Plasma-specific elements.
- **GTK3 `GetAccessibleAtPoint`**: May return null for some GTK3 apps that don't implement the `Component` interface correctly.
- **No permissions model**: All permission methods return `true`. Screen capture may need compositor-specific Portal permissions.