---
name: forepaw
description: Control desktop apps (macOS, Windows, Linux) for the user. Use when asked to interact with GUI applications, click buttons, fill forms, read screen content, or automate any desktop task. Read this before running any forepaw command.
---

# Desktop Automation with forepaw

`forepaw` is a CLI tool for controlling desktop applications. It works through bash -- every command goes through the permission gate.

## Platform Support

| Capability | macOS | Windows | Linux |
|---|---|---|---|
| List apps/windows | ✅ | ✅ | ✅ |
| List displays | ✅ | ✅ | ❌ |
| Snapshot (AX tree) | ✅ | ✅ (UIA) | ✅ (AT-SPI2) |
| Screenshot | ✅ | ✅ | ❌ |
| OCR | ✅ (Vision) | ✅ (WinRT) | ❌ |
| Hit test | ✅ | ✅ | ✅ |
| Click, type, press, scroll, drag, hover | ✅ | ❌ | ❌ |
| Permissions check | ✅ | ✅ (always yes) | ✅ (always yes) |

Observation works on all three platforms. Actions are macOS-only with stubs on Windows/Linux (clear error messages, not crashes). Cross-platform actions are coming.

## Global flags

Every command supports these:

- `-f json` / `--format json` — structured JSON output
- `-v` / `--verbose` — show native role, identifier, uid, signature in output
- `--version` — binary shows git SHA: `forepaw 0.4.0 (abc1234)`

Debug logging: `FOREPAW_LOG=debug` or per-module like `FOREPAW_LOG=snapshot=debug,app=info`. Falls back to `RUST_LOG`. Defaults to `warn`.

## Core loop

**observe -> decide -> act -> observe**

Always snapshot or screenshot before acting. Never assume UI state from a previous command -- the UI may have changed.

## Observation (pick the right one)

### 1. Accessibility tree (prefer this)

```bash
forepaw snapshot --app "App Name" -i         # interactive elements only (skips menus + hidden + offscreen)
forepaw snapshot --app "App Name" -i --diff  # diff against previous snapshot
forepaw snapshot --app "App Name" -i --menu  # include menu bar (excluded by default with -i)
forepaw snapshot --app "App Name" --compact  # remove empty structural containers
forepaw snapshot --app "App Name" --timing   # show per-subtree timing breakdown on stderr
```

Returns structured text with `@e` refs and window-relative positions:
```
app: Finder  window: [312,139 1010x614]
  button @e1 "Back" (10,4 60x30)
  textfield @e2 "Search" value="" (200,4 300x30) focused
  button @e3 "OK" disabled (500,300 80x30)
  list (10,44 1180x700)
    cell @e4 "README.md" selected (10,44 1180x25)
    cell @e5 "src" (10,69 1180x25)
```

The header line shows app name and window bounds (screen coordinates). Element state is shown inline: `disabled`, `focused`, `selected`. Enabled elements don't show anything (too noisy). Use `-v` (verbose) to see element descriptions, native roles (`AXButton`, `UIA 50000`), identifiers (`AutomationId`), uid, and signature.

All coordinates are **window-relative**: `(0,0)` is the window's top-left corner. These match what action commands expect. Coordinates are portable across window positions.

Best for: native macOS apps (Finder, System Settings, Notes, Xcode). For browsers, the full tree (without `-i`) includes web content elements like links, headings, and text.

**Electron apps (Discord, Slack, VS Code, Cursor, Notion, Linear, etc.)** are automatically detected. forepaw sets `AXManualAccessibility` to tell Chromium to expose its web content tree. The first snapshot of an Electron app may take an extra 1-3s while the tree builds; subsequent snapshots are fast. No special flags needed.

**Electron icon naming:** Electron apps using icon libraries (Lucide, Tabler, FontAwesome, etc.) get automatic icon names from CSS classes. An unnamed button with a Lucide settings icon becomes `button @e5 "settings"`. Also checks AXHelp, AXPlaceholderValue, and AXRoleDescription for additional names. Try `snapshot -i` first on any Electron app -- the tree is often better than expected.

**CEF apps (Spotify, Steam):** Apps using Chromium Embedded Framework (not Electron) expose zero AX tree content. `snapshot` returns only window chrome. These apps are **OCR-only** -- use `ocr`, `ocr-click`, `screenshot`, and coordinate-based `click` to operate them. See "Icon-only buttons in CEF apps" below.

**Multi-process apps (Steam):** Some apps render their UI in a helper process (e.g. `Steam Helper`). forepaw automatically discovers these windows when the main process has none -- just use `--app Steam` normally.

**Performance:** Offscreen elements (outside the visible window area) are automatically excluded in all modes. With `-i`, menu bar and zero-size (hidden/collapsed) elements are also excluded. This dramatically speeds up apps like Music that expose large amounts of invisible content. Use `--offscreen` to include offscreen elements, `--menu` or `--zero-size` to include those back with `-i`.

### 2. Hit test (quick element lookup)

```bash
forepaw hit-test 500,300                                  # what element at these screen coords?
forepaw hit-test 50,15 --app konsole                       # scoped to a specific app
forepaw hit-test 500,300 --json                            # machine-readable output
forepaw hit-test 500,300 --full-values                     # show entire element value (no truncation)
```

Finds the deepest accessibility element at screen coordinates. Returns role, name, value, bounds, available actions, owning PID, and ancestor chain (root → window → parent → element). Default truncates long values to 200 chars; use `--full-values` to see everything.

System-wide by default. Use `--app` to scope to a specific application. On macOS/Windows, this is a native hit test under 1ms. On Linux, it uses AT-SPI2's `Component.GetAccessibleAtPoint`.

### 3. OCR (fallback for sparse trees)

```bash
forepaw ocr --app Discord                    # all text with coordinates + screenshot
forepaw ocr --app Discord --find "Settings"  # filter for specific text
forepaw ocr --app Discord --no-screenshot    # text only, no screenshot saved
```

Returns a screenshot path (first line) followed by recognized text with click coordinates. The screenshot uses the best available format (WebP preferred, else JPEG) at 1x scale. Use when `snapshot` returns unnamed elements or when you need text that isn't in the AX tree.

**OCR replaces separate screenshot + OCR calls.** Since OCR already captures a screenshot internally for text recognition, it saves and returns that screenshot automatically. No need to run `screenshot` + `ocr` separately.

Screenshot format options: `--image-format`, `--quality`, `--scale`, `--no-cursor` (same as `screenshot` command).

### 4. Screenshot (for visual inspection)

```bash
forepaw screenshot --app "App Name"               # plain screenshot
forepaw screenshot                                 # full screen
forepaw screenshot --app "App Name" --ref @e5      # crop to element bounds
forepaw screenshot --app "App Name" --ref @e5 --padding 40  # more context around element
forepaw screenshot --app "App Name" --region 10,50,400,300  # crop to window-relative region (x,y,w,h)
forepaw screenshot --app "App Name" --grid 100     # overlay coordinate grid
```

Returns a screenshot path. Use when you need to see what's on screen without OCR text. The image can be read with the `read` tool.

**Area capture with `--ref` or `--region`:** Crops to the specified area. `--ref @eN` resolves the element's bounds from the AX tree. `--region x,y,w,h` uses window-relative coordinates. Both add 20px padding by default (override with `--padding`). Works with `--annotate` too -- annotations are rendered on the full image first, then cropped. Requires `--app`.

### 5. Annotated screenshot (visual + structural)

```bash
forepaw screenshot --app "App Name" --annotate           # numbered badges (default)
forepaw screenshot --app "App Name" --style spotlight     # dims non-interactive areas
forepaw screenshot --app "App Name" --style spotlight --only @e5 @e8 @e12  # highlight specific refs
```

Overlays numbered labels on interactive elements. Each label maps to an `@e` ref. Prints a legend:
```
[1] @e1 Button "Save"
[2] @e3 TextField "Search"
[3] @e5 CheckBox "Enable"
```

Styles: `badges` (minimal numbered pills, best for agents), `labeled` (bounding boxes with role+name), `spotlight` (dims non-interactive). Use when you need visual context for spatial layout. Prefer `snapshot -i` for most tasks -- it's faster and cheaper in tokens.

## Snapshot diffing

After performing an action, use `--diff` to see what changed instead of reading the full tree:

```bash
forepaw snapshot --app Finder -i              # baseline (auto-cached)
forepaw click @e3 --app Finder                # action
forepaw snapshot --app Finder -i --diff       # shows +/- of what changed
```

Output uses `+` for added lines and `-` for removed lines:
```
[diff: 3 added, 1 removed, 42 unchanged]

-   window "Documents" (0,0 1024x678)
+   window "Recents" (0,0 1024x678)
- button "Add Tags" (759,0 40x52)
+ button "Edit Tags" (759,0 40x52)
+ button "New Item" (500,300 80x30)
```

Ref shifts are handled automatically -- if a new element appears early in the tree and bumps all subsequent refs, unchanged elements still show as unchanged (refs are stripped for comparison, then the new refs are shown in the output).

Use `--context N` to show N unchanged lines around each change. The previous snapshot is cached per app in a temp file. No manual baseline management needed.

## Actions (macOS only)

### Click by ref (from snapshot)

```bash
forepaw click @e3 --app "App Name"
forepaw click @e3 --app "App Name" --right    # right-click (context menu)
forepaw click @e3 --app "App Name" --double   # double-click
```

### Click by coordinates (from snapshot bounds)

```bash
forepaw click 500,300 --app "App Name"    # click at window-relative position
forepaw hover 500,300 --app "App Name"    # hover at window-relative position
```

Coordinates are **window-relative** (0,0 = top-left of window). Use when you have coordinates from snapshot bounds but no ref. Read the `(x,y WxH)` from snapshot output and compute the center: `x + W/2, y + H/2`.

### Click/hover by region (for icon buttons without AX or text)

```bash
forepaw click 310,420,80,70 --app Spotify  # find & click prominent element in region
forepaw hover 325,410,60,60 --app Spotify  # find & hover prominent element (triggers tooltips)
```

Pass 4 values `x,y,w,h` to target a rough area. forepaw captures a screenshot, analyzes pixel saturation in that region, finds the centroid of the most colorful element, and clicks/hovers it. Ideal for icon-only buttons in CEF apps (play, shuffle, close) where there's no AX ref and no text for OCR. The region doesn't need to be precise -- a box that contains the target works.

**Without --app**, `hover` treats coordinates as screen-absolute (for global positioning). All other coordinate commands require `--app`.

### Click by text (from OCR)

```bash
forepaw ocr-click "Button Label" --app Discord
forepaw ocr-click "file.txt" --app Finder --double   # double-click
forepaw ocr-click "item" --app "App Name" --right    # right-click
```

`--right` and `--double` work on both `click` and `ocr-click`. When multiple matches are found, `ocr-click` errors with a listing and you pick with `--index N`. Single matches click without needing `--index`. Prefer `click @ref` when available -- it's unambiguous.

### Type into element (from snapshot) -- preferred

```bash
forepaw type @e2 "search query" --app "App Name"
```

Focuses the element via AX, then types. More reliable than `keyboard-type` because it ensures the right element receives input -- some text fields need AX focus, not just a mouse click.

### Type into current focus (no ref needed)

```bash
forepaw keyboard-type "hello world" --app "App Name"  # activates app first
forepaw keyboard-type "hello world"                     # types into current focus
```

Use when there's no AX ref for the target (e.g. inside batch after a coordinate click). Prefer `type @ref` when a ref is available.

### Keyboard shortcuts

```bash
forepaw press cmd+s --app "App Name"   # activates app first
forepaw press opt+space                 # global hotkey (no --app)
```

### Drag (drawing, moving, resizing)

```bash
forepaw drag 100,100 500,500 --app "App Name"                         # simple drag
forepaw drag 100,100 300,200 500,100 700,300 --app "App Name"         # multi-point path
forepaw drag @e3 @e7 --app "App Name"                                 # between two elements
forepaw drag 100,100 500,500 --app "App Name" --steps 60 --duration 1.0  # slower, smoother
forepaw drag 100,100 500,350 --app "App Name" --modifiers shift       # constrained (45-degree)
forepaw drag 100,100 500,500 --app "App Name" --modifiers shift+alt   # combined modifiers
forepaw drag 100,100 300,200 500,100 --app "App Name" --close         # auto-close path to start
forepaw drag 100,100 500,500 --app "App Name" --right                 # right-button drag
forepaw drag 100,100 500,500 --app "App Name" --pressure 0.5          # tablet pressure
echo "100,100 200,150" | forepaw drag --stdin --app "App Name"        # stdin mode
```

Drags the mouse with smooth interpolation. Supports coordinates, refs, or a mix. For paths with 3+ points, all must be coordinates.

- `--steps` controls smoothness per segment (default 30, higher = smoother)
- `--duration` controls total drag time in seconds (default 0.3)
- `--modifiers shift+alt` holds modifiers during the entire drag
- `--close` appends start point to end of path (3+ points only)
- `--right` uses right mouse button
- `--pressure 0.0-1.0` sets mouse pressure
- `--stdin` reads coordinates from stdin (space or newline separated x,y pairs)

### Scroll

```bash
forepaw scroll down --app Orion              # scroll down 3 ticks (default)
forepaw scroll up --app Orion --amount 10    # scroll up 10 ticks
forepaw scroll left --app Finder             # horizontal scroll
forepaw scroll down --app Orion --ref @e5    # scroll within a specific element
forepaw scroll down --app Discord --at 200,400  # scroll at window-relative coordinates
```

Directions: `up`, `down`, `left`, `right`. Default amount is 3 ticks. Use `--at x,y` to scroll a specific panel or sidebar when no ref is available. Coordinates are window-relative and validated against window bounds.

**Boundary detection:** When scroll hits the edge, the result message includes `(at boundary -- content did not change)`. Stop scrolling in that direction when you see this.

### Hover (trigger tooltips/hover states)

```bash
forepaw hover @e5 --app "App Name"              # by ref (from snapshot)
forepaw hover "Submit" --app "App Name"          # by text (OCR lookup)
forepaw hover 200,470 --app Discord              # at window-relative coordinates
forepaw hover 325,410,60,60 --app Spotify       # region-based (saliency)
forepaw hover 700,400 --app Orion --smooth      # smooth mouse movement
```

Accepts an `@e` ref, text (OCR lookup), coordinates, or a region (`x,y,w,h`). Useful for triggering tooltips, hover menus, or preview popups.

**`--smooth` flag:** Moves the mouse along a path from current position to target with intermediate events, instead of teleporting. Some apps need this to register mouse leave events (e.g. Orion's auto-hiding tab sidebar).

**Tooltip discovery for unnamed elements:** Some apps (especially Discord) have icon-only buttons with no AX name. Hover at their coordinates to trigger a tooltip, then snapshot -- the tooltip appears in the AX tree as `subrole=AXUserInterfaceTooltip` with the element's name.

### Wait (poll for text to appear)

```bash
forepaw wait "Loading complete" --app "App Name"                # default: 10s timeout, 1s interval
forepaw wait "Submit" --app "App Name" --timeout 30 --interval 2  # custom timing
```

Polls via OCR until the text appears on screen. Throws an error on timeout. Use after actions that trigger async UI changes (navigation, loading, dialog appearance).

### Batch (multiple actions in one call)

```bash
forepaw batch --app Notes "click @e3 ;; keyboard-type hello ;; press return"
forepaw batch --app Finder --delay 200 "click @e1 ;; wait \"Documents\" ;; click @e5"
```

Executes actions sequentially, separated by `;;`. The `--app` and `--window` flags apply to all actions unless overridden per-action. Default 100ms delay between actions (configurable with `--delay`).

Supported actions: `click`, `drag`, `hover`, `type`, `keyboard-type`, `press`, `scroll`, `ocr-click`, `wait`.

**Use batch for any multi-step interaction.** Separate CLI invocations return control to the terminal between commands, which steals focus from the target app. Batch keeps the app focused throughout the entire sequence. This is essential for click-then-type patterns, browser URL bar entry, and any sequence where focus must be maintained.

### Newlines in text input

Use `press shift+return` between lines:
```bash
forepaw keyboard-type "first line" --app Discord
forepaw press shift+return --app Discord
forepaw keyboard-type "second line" --app Discord
forepaw press return --app Discord    # send
```

## Window targeting

When an app has multiple windows, use `--window` to target a specific one:

```bash
forepaw list-windows --app Zed
# w-1234  Zed  "my-project"
# w-1235  Zed  "other-project"

forepaw screenshot --app Zed --window "my-project"    # by title substring
forepaw screenshot --app Zed --window-id 1234          # by numeric ID
forepaw scroll down --app Zed --window "my-project"   # works with scroll too
forepaw ocr --app Zed --window "my-project"            # and OCR
forepaw ocr-click "text" --app Zed --window "my-project"  # and ocr-click
```

Without `--window`, commands target the largest window for that app.

The title shown in quotes in `list-windows` output is what you pass to `--window`. If the title matches multiple windows, forepaw returns an error listing all matches with their IDs.

## When to use --app / --pid / --window / --window-id

- **With --app**: activates the app by name before acting. Use for click, type, keyboard-type, press when targeting a specific app.
- **With --pid**: activates the app by process ID. Use when you need unambiguous targeting (multiple instances, similar names). PIDs are shown in `list-apps` output.
- **With --window**: targets by window title (case-insensitive substring match). Use when an app has multiple open windows.
- **With --window-id**: targets by numeric window ID from `list-windows`. Accepts bare IDs (`1234`) and w-prefixed (`w-1234`).
- **Without any**: sends input globally. Use for system hotkeys (Raycast, Spotlight) or typing into whatever is already focused.

`--app` and `--pid` are mutually exclusive. `--window` and `--window-id` are mutually exclusive. Use `--app` by default; switch to `--pid` when name resolution is ambiguous.

## Important behaviors

- **Always observe before acting.** Don't guess UI state.
- **Refs are positional.** `@e3` means "the 3rd interactive element in depth-first order." If the UI changes (menu opens, dialog appears), refs shift. Re-snapshot after any action that changes the UI.
- **Snapshot activates the app.** The snapshot command brings the app to the foreground so the AX tree matches what action commands will see. Some apps (especially browsers) expose different elements when active vs. background.
- **Prefer `type @ref` over click + keyboard-type.** `type` focuses the element via AX and types into it directly. `keyboard-type` after a click can fail if the click didn't give the element AX focus. Use `keyboard-type` only inside batch (after coordinate clicks) or when no ref is available.
- **Use batch for multi-step interactions.** Separate CLI invocations return control to the terminal, which steals focus from the target app. Any click-then-type or multi-action sequence should use batch.
- **AX tree vs OCR.** Try `snapshot -i` first. Electron apps are auto-detected and their web content trees are enabled automatically. If the tree is still sparse, fall back to OCR.
- **App activation.** `--app` brings the app to the foreground. This means the user's screen will change. Warn them before switching apps if they didn't explicitly ask.
- **Mouse clicks are physical.** OCR-click and mouse-fallback clicks move the actual cursor and click on screen. The user will see this happening.
- **Coordinates are window-relative.** All coordinates in snapshots, OCR output, and action commands are relative to the window's top-left corner (0,0). This means coordinates don't change when the window moves. When `--app` is specified, `click` and `hover` validate that coordinates are within window bounds. Without `--app`, `hover` uses screen-absolute coordinates.
- **Keystroke delay.** Typing is not instant (~8ms per character). Long text takes a moment.
- **Wait timeout.** `wait` polls via OCR (screenshot + text recognition each poll). Keep intervals reasonable (1s+) to avoid hammering the system.
- **Text starting with dashes.** If text for `keyboard-type`, `type`, `ocr-click`, or `wait` starts with `-` or `--`, use the `--text` option instead of a positional argument:
  ```bash
  forepaw keyboard-type --text "--this starts with dashes" --app Notes
  forepaw type @e5 --text "-dash text" --app Notes
  forepaw ocr-click --text "--Settings" --app App
  ```
  `--text` unconditionally takes the next argument as its value, even if it looks like a flag.

## Icon-only buttons in CEF apps

CEF apps (Spotify, Steam) have no AX tree. OCR finds text but not icon buttons (play, skip, heart, gear). Use these techniques:

### Region click/hover (preferred for icon buttons)
Pass a 4-component target `x,y,w,h` to `click` or `hover` to target a rough area. forepaw finds the most visually prominent element by pixel saturation and clicks/hovers its center:
```bash
forepaw click 310,420,80,70 --app Spotify   # clicks green play button in that region
forepaw hover 325,410,60,60 --app Spotify   # hovers play button, triggers tooltip
```
The agent provides a rough bounding box; forepaw handles pixel-level targeting. Works because colored UI elements have high saturation against desaturated backgrounds.

Output includes the detected coordinates: `clicked prominent element at 349,453 (in region 310,420 80x70)`.

### General strategy for CEF apps
1. Use `ocr-click` for all text-labeled controls (tabs, menu items, links, song titles)
2. Use `click x,y,w,h` (region click) for icon-only buttons -- draw a rough box around the target
3. Use `ocr-click "text" --double` to activate list items (play songs, open files)
4. Use keyboard shortcuts when available (`space` for play/pause in media apps)
5. Use `hover` + screenshot to discover interactive regions via hover states

## Permissions

If commands fail with permission errors:
```bash
forepaw permissions          # check status
forepaw permissions --request  # trigger system dialogs
```

macOS needs two permissions:
- **Accessibility** (System Settings > Privacy & Security > Accessibility) -- for snapshot, click, type
- **Screen Recording** (System Settings > Privacy & Security > Screen & System Audio Recording) -- for screenshot, ocr, ocr-click

Windows and Linux don't need permission setup (UIA/AT-SPI2 work without gates).

## Discovering apps and windows

```bash
forepaw list-apps                  # running GUI apps
forepaw list-windows --app Finder  # windows for an app
```

`list-apps` shows the frontmost (active) app with a trailing `*`:
```
Finder (com.apple.finder) [pid: 1374]
Ghostty (com.mitchellh.ghostty) * [pid: 1331]
```

`list-windows` shows window bounds:
```
w-42  Finder  "Documents"  [312,139 1010x614]
```

Use the exact app name from `list-apps` in `--app` flags. Use `list-windows` to find window titles/IDs for `--window`/`--window-id`.

## Discovering displays

```bash
forepaw list-displays              # monitors: scale, bounds, color space, Hz
```

```
1 Built-in Retina Display * (builtin)  [0,0 1800x1169]  2.0x  Color LCD  120 Hz
4 Sidecar Display (AirPlay)           [1800,420 1295x876]  2.0x  Sidecar Display  60 Hz
```

`*` marks the primary display, `(builtin)` marks a laptop panel. The `2.0x` is the backing scale factor -- multiply logical sizes by it to get pixel dimensions. Use this when you need to know the actual scale a screenshot was captured at (multi-display setups with different per-monitor scales), or to reason about display layout. macOS and Windows implemented; Linux stubbed.