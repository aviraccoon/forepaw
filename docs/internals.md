# Internals

How forepaw works under the hood. For performance numbers, see `performance-macos.md`.

## The accessibility tree

> **The raccoon version:** Imagine a raccoon feeling around a trash can in the dark. It doesn't see the lid, the bag, the food -- it feels shapes, edges, handles. That's the accessibility tree. Every app on macOS exposes a tree of UI elements -- buttons, text fields, labels -- that screen readers (and raccoons like us) can feel around in without looking at pixels.

`AXUIElementCreateApplication(pid)` gives a handle to an app's UI hierarchy. From there:

- `AXUIElementCopyMultipleAttributeValues` reads attributes in batch (13 per element in one IPC call)
- `AXUIElementCopyAttributeValue` reads individual attributes (only used for `AXTitleUIElement` targets, which are different `AXUIElement` handles)
- `AXUIElementPerformAction` performs actions (`AXPress`, `AXRaise`)
- `AXUIElementSetAttributeValue` sets values directly (`AXValue`, `AXFocused`, `AXManualAccessibility`)

The tree is walked depth-first with a configurable max depth (default 15, 25 for Electron apps). Each interactive element gets a positional ref (`@e1`, `@e2`, ...) based on its depth-first order.

### Batched attribute fetching

> **The raccoon version:** A raccoon could pick up one french fry at a time, or it could grab the whole container. We grab the whole container. One trip to the trash can per item instead of thirteen.

`AXUIElementCopyMultipleAttributeValues` fetches 13 attributes per element in a single Mach IPC round-trip:

| Index | Attribute | Purpose |
|-------|-----------|--------|
| 0-7 | Role, Title, Description, Value, Position, Size, Children, Subrole | Core tree structure |
| 8-12 | TitleUIElement, Help, PlaceholderValue, DOMClassList, RoleDescription | Name resolution fallbacks |

Indices 8-12 were previously fetched individually in `computedName`, causing up to 5 extra IPC calls per unnamed element. Batching them eliminates those calls. The impact is dramatic on slow AX responders (Music: ~50s to ~12s, Electron apps: 1.5-1.9x faster).

### Element name resolution

> **The raccoon version:** Not everything in the trash is labeled. Sometimes there's a tag on the container, sometimes you have to open it and sniff the contents, sometimes you recognize the shape. forepaw tries six different ways to figure out what an element is called, from most to least reliable.

Many elements don't expose a direct `AXTitle`. The resolution chain, applied when both `AXTitle` (batch index 1) and `AXDescription` (batch index 2) are empty:

1. **AXTitleUIElement** (index 8) -- a reference to a separate label element. Read that element's `AXValue` or `AXTitle`. This is the only step that makes individual IPC calls (2 calls max), because the referenced element is a different `AXUIElement` whose attributes aren't in our batch.
2. **First child scan** -- check pre-built child `ElementNode` objects for the first `AXStaticText` child's value or `AXImage` child's computed name. No IPC needed: `buildTree` recurses children before computing the parent's name, so children are already fully built.
3. **AXHelp** (index 9) -- descriptive help text.
4. **AXPlaceholderValue** (index 10) -- text field placeholder.
5. **AXDOMClassList** (index 11) -- CSS class list, parsed by `IconClassParser` to extract icon names from Lucide, Tabler, FontAwesome, Material, Heroicons, Phosphor, Bootstrap, Feather, Ionicons, Octicons, Codicons prefixes.
6. **AXRoleDescription** (index 12) -- only when it's more specific than the generic role description (e.g., not just "button" or "text field"). A set of ~35 generic descriptions is filtered out.

The children-first build order is key: `buildTree` recurses into children, builds their `ElementNode` objects (which triggers their own name resolution), then computes the parent's name using those already-built children. Step 2 reads from in-memory objects instead of making IPC calls per child.

### Tree pruning

> **The raccoon version:** A smart raccoon doesn't dig through the recycling bin when the pizza box is right on top. forepaw skips parts of the tree that can't possibly contain anything useful -- stuff that's off-screen, invisible, or in the menu bar. Less digging, faster meals.

Three independent pruning mechanisms, applied during the tree walk:

**Offscreen pruning** (default-on): Skips subtrees whose bounds are entirely outside the window rect. The intersection test compares element bounds (screen-absolute) against window bounds (screen-absolute). Only applied at depth > 2 to preserve window/container structure. Elements without bounds or with zero size are not pruned. This is what makes Apple Music usable -- it exposes 800+ invisible play history rows at negative Y coordinates, and pruning skips them all.

**Zero-size pruning** (`-i` mode): Skips subtrees rooted at 0x0 elements -- collapsed menus, hidden panels, offscreen content with no spatial extent. Only at depth > 1.

**Menu bar pruning** (`-i` mode): Skips the `AXMenuBar` subtree entirely. Menu bars contain 200-300 elements that agents rarely need.

All three are bypass-able: `--offscreen`, `--zero-size`, `--menu` include them back.

### Electron app detection

> **The raccoon version:** Some trash cans have a second, hidden compartment that only opens if you press a secret latch. Electron apps are like that -- they have a full accessibility tree inside, but they don't build it unless you ask nicely. forepaw presses the latch.

forepaw detects Electron apps by checking for `Contents/Frameworks/Electron Framework.framework` in the app bundle. When detected:

1. Sets `AXManualAccessibility` on the app element -- Chromium's official API for third-party assistive technology, the same signal VoiceOver sends.
2. Polls for up to 3 seconds until the `AXWebArea` has interactive children (Chromium needs time to build the tree after first enable).
3. Uses depth 25 instead of 15 (Electron's DOM nesting creates 13+ levels of groups).

`AXManualAccessibility` is idempotent and is also set during `resolveRef` (action dispatch), so clicking works without a preceding snapshot.

Electron apps using icon libraries (Lucide, Tabler, FontAwesome, etc.) get automatic icon names via `IconClassParser`, which strips known CSS class prefixes. An unnamed button with class `lucide-settings` becomes `button "settings"`.

### Ref system

> **The raccoon version:** Raccoons remember routes by sequence -- third tree, second fence, first dumpster. forepaw numbers every interactive element in the order it finds them. `@e3` means "the 3rd interactive thing I touched on my walk through the tree."

Refs are assigned by `RefAssigner` (ForepawCore) which walks the tree depth-first and assigns sequential numbers to interactive elements. The interactive role set: button, text field, text area, checkbox, radio button, slider, combo box, popup button, menu button, link, menu item, tab, switch, incrementor, color well, tree item, cell, dock item.

During `buildTree` (DarwinProvider), `AXUIElement` handles are collected into a `[Int: AXUIElement]` map at the same positions that `RefAssigner` will use. After assignment, the map is transferred to `refTable` keyed by `ElementRef`.

### Ref resolution across invocations

Each CLI invocation creates a fresh `DarwinProvider`. Refs from `snapshot` are just positional numbers. When `click @e10 --app Finder` runs, `resolveRef` re-walks the tree with `collectAXElements` (a lightweight walk that only fetches role + children, no batching) counting interactive elements until it hits the 10th one.

This works as long as the UI hasn't changed between snapshot and action. If the UI did change (menu opened, dialog appeared), the ref is stale and may point to a different element.

`resolveRef` uses the same depth as snapshot (15 for native, 25 for Electron) to ensure refs are consistent.

### Action dispatch

**Click**: For most roles, tries `AXPress` first (the accessibility action). For `AXLink` elements, right-clicks, and double-clicks, uses mouse click directly -- browsers don't navigate on `AXPress` for web content links, and AXPress can't express right-click or double-click. Falls back to CGEvent mouse click at the element's center coordinates.

**App activation**: Before any mouse click or keystroke targeting an app, `NSRunningApplication.activate()` is called with a 300ms delay. CGEvent posts to whatever window is under the cursor, so the target app must be frontmost. Without activation, clicks go to the wrong window. This 300ms delay is the largest fixed cost in action dispatch.

**Type**: Tries `AXUIElementSetAttributeValue` on the element's value first. Falls back to focusing the element via `AXRaise` + `AXFocused` and simulating keystrokes.

## Window-relative coordinates

> **The raccoon version:** If someone moves the trash can, the pizza box is still in the same spot *inside* the can. forepaw describes everything relative to the window's top-left corner, so coordinates don't break when the window moves.

All coordinates in forepaw are window-relative: `(0,0)` is the window's top-left corner.

**Conversion boundary**: The relative-to-absolute conversion happens at the provider layer (`DarwinProvider`), not the CLI layer. The CLI passes window-relative coordinates through; `DarwinProvider.toScreenPoint()` adds the window origin before CGEvent calls.

**Snapshot output**: `TreeRenderer` subtracts the window origin (`windowBounds` on `ElementTree`) from element bounds. Elements store screen-absolute bounds internally (as reported by AX), but display window-relative.

**OCR coordinates**: The window screenshot (`screencapture -l`) has `(0,0)` at the window's top-left in image-pixel space. After Vision's bottom-left normalization is undone, coordinates are divided by `backingScaleFactor` (typically 2.0) to get logical window-relative coordinates. No window origin adjustment needed since the image is already window-scoped.

**Coordinate validation**: `CoordinateValidation.validate(point:windowSize:)` checks `0..width, 0..height`. Applied to `click`, `hover`, `scroll --at`, and `drag` when `--app` is specified. Rejects out-of-bounds coordinates with an error (not a clamp -- a misplaced click could be destructive).

**Exception**: `hover` without `--app` treats coordinates as screen-absolute (for global positioning like Spotlight/Raycast).

## Window resolution

`DarwinProvider.findWindow(pid:window:)` resolves which window to target:

1. If `window` starts with `w-`, match by `CGWindowID`.
2. If `window` is provided, case-insensitive substring match against window titles.
3. Otherwise, pick the largest non-phantom window (>= 10px) preferring titled windows.

The titled-window preference avoids Finder's full-screen desktop window (larger but untitled) and similar invisible windows.

Window info comes from `CGWindowListCopyWindowInfo(.optionOnScreenOnly, ...)`. The `ResolvedWindow` struct wraps the window ID, title, and bounds dict.

### Multi-process app fallback

> **The raccoon version:** Some trash cans are two-part: one bin for the lid, another for the bag. Steam is like that -- the app you see in the Dock has no real windows, while a hidden helper process holds the actual UI. forepaw checks both bins.

Some apps (Steam) render their UI in a helper process with `accessory` activation policy. The main process (shown in `list-apps`) has only phantom windows (1x1 at offscreen coordinates). When `findWindow` finds no usable windows for the main PID, it falls back to searching `CGWindowList` for onscreen windows owned by processes with the same bundle ID prefix (e.g., `com.valvesoftware.steam` matches `com.valvesoftware.steam.helper`). This enables screenshots, OCR, and coordinate-based actions on Steam and similar multi-process apps.

### CEF vs Electron

CEF (Chromium Embedded Framework) apps like Spotify and Steam use the same Chromium engine as Electron but don't respond to `AXManualAccessibility`. The CEF accessibility bridge requires each embedding application to implement it -- unlike Electron, which provides a universal activation mechanism. CEF apps are detected by `Chromium Embedded Framework.framework` in the bundle but are NOT treated as Electron apps. They are OCR-only.

## Region click (saliency detection)

> **The raccoon version:** A raccoon can't describe *exactly* where the shiny thing is in the garbage bag, but it knows which *part* of the bag it's in. It reaches into that area and grabs the most interesting thing it touches. Region click works the same way -- point at an area, and forepaw finds the shiniest thing in it.

`click x,y,w,h` targets a rough area instead of precise coordinates. `SaliencyDetector` captures a screenshot, crops the specified region, and finds the centroid of high-saturation pixels.

Why saturation? UI buttons are colored (green play, blue links, red close, orange warnings). Backgrounds are desaturated (gray, black, white). The most saturated pixels in a small region are almost always the target button.

### Pipeline

1. Capture window screenshot via `screencapture -l`
2. Crop to the specified region (in Retina pixel coordinates)
3. Render pixels into an RGBA buffer via `CGContext`
4. Compute HSL saturation per pixel; also track brightness deviation from median as fallback for desaturated icons (white on dark)
5. Weighted centroid of pixels above saturation threshold (0.25) or brightness deviation threshold (0.3)
6. Convert centroid from crop-pixel coordinates to window-relative logical coordinates
7. Click at the centroid via the standard mouse click path

The agent's job becomes "draw a rough box around the target" (which LLMs can do from screenshots) instead of "predict exact pixel coordinates" (which LLMs cannot reliably do -- see Anthropic's computer use research on pixel counting difficulty).

## OCR (Vision framework)

> **The raccoon version:** Sometimes the trash can is sealed and you can't feel inside -- you have to *look* at it. OCR takes a picture of the window and reads the text, like a raccoon squinting at a label through the plastic.

For apps where the accessibility tree has gaps, forepaw screenshots the window and runs `VNRecognizeTextRequest`.

### Pipeline

1. Capture full-resolution PNG via `screencapture -l <windowID>` (2x Retina)
2. Run `VNRecognizeTextRequest` (`.accurate`, no language correction)
3. Convert Vision's normalized bottom-left coordinates to window-relative logical pixels
4. Optionally save an agent-friendly display copy (WebP/JPEG, 1x)

The `ocr` command combines screenshot and OCR into one operation, returning the display screenshot path alongside text results.

### Text search

When `--find` is specified, `findPrecise` uses Vision's `candidate.boundingBox(for:)` to get word-level bounding boxes for the matched substring within larger text blocks. This gives precise click coordinates for individual words inside a paragraph. Falls back to block-level filtering if precise matching finds nothing.

### OCR settings

- Recognition level: `.accurate` (not `.fast`) for reliability
- Language correction: disabled -- preserves usernames, IDs, and technical text that autocorrect would mangle

## Input simulation

> **The raccoon version:** Raccoons don't just observe -- they manipulate. Twist lids, pull latches, press buttons. forepaw simulates keyboard and mouse input through macOS's CGEvent system, which is like having invisible raccoon paws that the OS can't distinguish from real human input.

### Keyboard (CGEvent)

Keystrokes are synthesized via `CGEvent(keyboardEventSource:virtualKey:keyDown:)`. For text input, each character is sent as a unicode string on virtual key 0 with `keyboardSetUnicodeString`. Named keys (return, escape, arrows, function keys) map to virtual key codes via `KeyCodeMap`.

Modifier keys (cmd, shift, opt, ctrl) are set via `CGEventFlags` on the key event.

**Inter-character delay**: 8ms between keystrokes. Electron apps (Discord, Slack, VS Code) have async input handling that drops characters if events arrive too fast. Native macOS apps handle any speed fine, but the delay is always applied for consistency.

### Mouse (CGEvent)

Mouse clicks use `performMouseClick`: move cursor to target first (via `mouseMoved` event with 50ms settle time), then `leftMouseDown` + `leftMouseUp`. The pre-move ensures the click routes to the correct window.

Double-click uses `mouseEventClickState` to signal the click sequence number. Right-click uses `rightMouseDown` + `rightMouseUp`.

**Hover**: `moveMouse` posts a single `mouseMoved` event with 50ms settle time. `smoothMoveMouse` interpolates 20 intermediate `mouseMoved` events over 150ms for apps that track `mouseEnter`/`mouseLeave` (e.g., Orion's auto-hiding sidebar). Without smooth movement, teleporting the cursor doesn't trigger tracking area events.

**Drag**: `performMouseDrag` moves to start, posts `mouseDown`, interpolates through path segments with `mouseDragged` events, then posts `mouseUp`. Steps per segment (default 30) and total duration (default 0.3s) control smoothness. Modifier keys and pressure are applied to every event in the drag.

### Scroll (CGEvent)

Scroll events use `CGEvent(scrollWheelEvent2Source:...)` with `.line` units. The mouse is moved to the target point first (via `moveMouseToScrollTarget`). Scroll amount is in "ticks" (lines).

**Boundary detection**: After scrolling, forepaw captures a pixel fingerprint of the window -- a 20px horizontal strip from the vertical center, excluding the rightmost 30px to avoid transient scrollbar overlays -- using `CGWindowListCreateImage`. If the fingerprint matches the pre-scroll capture, the scroll hit a boundary and the message says so.

## Snapshot diffing

> **The raccoon version:** A raccoon returns to the same dumpster each night and immediately notices what changed -- new bags, missing boxes. Snapshot diffing does the same: compare before and after to see exactly what moved.

`SnapshotDiffer` compares two rendered snapshot texts using LCS (longest common subsequence) diff. Refs are stripped before comparison (`@e5` removed from lines) so positional ref shifts from added/removed elements don't produce false changes. The output shows the new refs.

Snapshots are cached per-app in temp files (`/tmp/forepaw-snapshot-<app>.txt`) via `SnapshotCache`. No manual baseline management -- `snapshot` caches automatically, `snapshot --diff` loads the previous and compares.

## Annotated screenshots

> **The raccoon version:** Sometimes you need a map of the dumpster. Annotated screenshots overlay numbered labels on every interactive element, like someone put little flags on every lid, handle, and latch so you can say "open flag 3" without fumbling around.

The annotation pipeline is split across targets:

1. **`AnnotationCollector`** (ForepawCore) walks the element tree and collects `Annotation` structs for interactive elements with bounds. Converts to window-relative coordinates and filters off-screen elements.
2. **`AnnotationRenderer`** (ForepawDarwin) draws on the image via CoreGraphics. Three styles: `badges` (numbered pills), `labeled` (bounding boxes with role+name), `spotlight` (dims non-interactive areas). Color-coded by `AnnotationCategory`: green=buttons, yellow=text fields, blue=selection controls, purple=navigation.
3. **`AnnotationLegend`** (ForepawCore) formats the text legend mapping display numbers to refs.

Annotations are rendered on the full window image, then cropped if `--ref` or `--region` is specified.

## Screenshot processing

All screenshots start as full-resolution PNG via `screencapture`. Post-processing handles format conversion and scaling:

1. **Scale** (1x mode): `sips --resampleWidth` halves the Retina image to logical pixels.
2. **Format**: WebP via `cwebp` (external binary, must be installed), JPEG via `sips`, or kept as PNG.
3. **Crop**: `--ref @eN` resolves element bounds, `--region x,y,w,h` uses window-relative coordinates. Both add padding (default 20px). Cropping uses CoreGraphics `CGImage.cropping(to:)`.

Default output: best available format (WebP if `cwebp` installed, else JPEG), 1x scale, cursor visible. Agent-optimized: ~85-150KB per window.

## Timing diagnostics

`--timing` on snapshot outputs a per-subtree breakdown to stderr. `SnapshotTiming.report()` adaptively expands subtrees holding >10% of total nodes and collapses single-child wrapper chains (common in Electron's deep DOM nesting). Output goes to stderr so it doesn't pollute the snapshot output that gets cached/diffed.

## Permissions

Two separate macOS permissions are required:

| Permission | Used by | API check |
|-----------|---------|----------|
| Accessibility | snapshot, click, type | `AXIsProcessTrusted()` |
| Screen Recording | screenshot, ocr, ocr-click | `CGPreflightScreenCaptureAccess()` |

Both are per-app (granted to the terminal, not to forepaw itself). After granting, the terminal may need a restart for the permission to take effect.

## Platform abstraction

> **The raccoon version:** Raccoons live everywhere -- cities, forests, suburbs. forepaw is designed the same way: the core logic (how to number elements, render trees, parse keys) is habitat-agnostic. Only the paws are specialized -- different grips for different dumpsters.

`ForepawCore` defines `DesktopProvider`, a protocol with no platform-specific types. All coordinates use `Point` and `Rect` (not `CGPoint`/`CGRect`). The macOS implementation (`ForepawDarwin/DarwinProvider`) converts to platform types internally.

ForepawCore contains:
- `DesktopProvider` protocol (all public API)
- `ElementTree`, `ElementNode`, `ElementRef` (tree types)
- `RefAssigner` (ref assignment, depth-first)
- `TreeRenderer` (text output with window-relative coords)
- `SnapshotDiffer`, `SnapshotCache` (diffing)
- `AnnotationCollector`, `AnnotationLegend` (annotation data)
- `IconClassParser` (CSS class to icon name)
- `CoordinateValidation` (bounds checking)
- `CropRegion` (area screenshot math)
- `KeyCombo` (key combo parsing)
- `OutputFormatter` (JSON/text output)

ForepawDarwin contains:
- `DarwinProvider` (the `DesktopProvider` implementation)
- `DarwinProvider+Snapshot` (AX tree walk, batching, pruning, name resolution)
- `DarwinProvider+Input` (click, type, press, scroll, hover, drag, wait)
- `DarwinProvider+Screenshot` (screencapture, OCR, annotations, cropping)
- `AnnotationRenderer` (CoreGraphics drawing)
- `OCREngine` (Vision framework)
- `KeyCodeMap` (virtual key codes)

A Linux implementation would use AT-SPI2 over DBus for the accessibility tree and uinput/libxdo for input simulation. The CLI, ref system, tree rendering, and output formatting would be identical.

## Known limitations

- **Retina coordinate math**: OCR coordinates assume the primary display's scale factor. Multi-display setups with different scale factors may produce incorrect click positions.
- **Menu timing**: Opening a dropdown menu changes the accessibility tree. A snapshot taken immediately after clicking a menu button may not include the menu items if the UI hasn't updated yet. Add a small delay between click and snapshot.
- **Web content in browsers**: AXPress on links doesn't trigger navigation in some browsers (confirmed in Orion). Mouse click works but requires app activation.
- **Text arguments starting with `--`**: ArgumentParser interprets `--` as end-of-options. Use the `--text` option (with `parsing: .unconditional`) instead of the positional argument.
- **Window-specific screenshots**: Uses `screencapture -l <windowID>` which captures the window as-is, including any overlapping windows. Not a clean window capture if other windows are on top.
- **Ref depth mismatch**: `resolveRef` uses depth 15 (native) or 25 (Electron). Using a non-default `--depth` on `snapshot` makes refs inconsistent with action commands.
- **Hover teleportation**: Without `--smooth`, hover teleports the cursor, which doesn't trigger `mouseEnter`/`mouseLeave` tracking areas. Some apps (e.g., Orion's sidebar) don't register the mouse leaving their hover zone.
- **Scroll fingerprinting**: Uses `CGWindowListCreateImage` (deprecated in macOS 14). Works but should migrate to ScreenCaptureKit when Apple removes it or deployment target is bumped.
- **VM guest typing**: `keyboard-type` sends wrong characters into VM guests (UTM). `press` commands work. VM hypervisors intercept CGEvent keystrokes differently.
- **CEF apps have no AX tree**: Spotify, Steam, and other CEF apps expose zero accessibility tree content. Only OCR, screenshots, and coordinate/region-based actions work. No `@e` refs.
- **Region click assumes colored targets**: `SaliencyDetector` uses pixel saturation to find button centers. Works well for colored buttons on desaturated backgrounds (most dark and light themes). May struggle with monochrome UIs where buttons and background have similar saturation.
