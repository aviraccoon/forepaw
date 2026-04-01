# forepaw

Desktop automation CLI for AI agents. Control any application through OS accessibility trees, OCR, and input simulation.

Named after the raccoon's dexterous forepaws -- precise manipulation of UI elements without brute force.

## How it works

Three observation strategies, used based on what the target app exposes:

1. **Accessibility tree** (best) -- structured text with `@e` refs. Works well for native macOS apps. Electron apps (Discord, Slack, VS Code, Cursor, Notion, Linear) are automatically detected and their Chromium accessibility trees enabled via `AXManualAccessibility`.
2. **OCR** -- screenshot + Vision framework text recognition. Fallback for apps where the accessibility tree is still insufficient.
3. **Annotated screenshots** -- numbered labels overlaid on interactive elements, bridging visual and structural. Three styles: badges (agent-optimized), labeled (human-readable), spotlight (focus mode).
4. **Plain screenshots** -- visual fallback for anything else. Can be sent to vision models.

```
forepaw snapshot --app Finder -i    # accessibility tree with refs
forepaw click @e3 --app Finder      # click element by ref
forepaw ocr --app Discord           # OCR the window, get text + coordinates
forepaw ocr-click "Settings" --app Discord  # find text and click it
forepaw keyboard-type "hello" --app Notes   # type into focused element
forepaw press cmd+s --app Finder    # keyboard shortcut
forepaw press opt+space             # global hotkey (no --app)
forepaw scroll down --app Finder    # scroll down 3 ticks
forepaw drag 100,100 500,500 --app Finder  # drag between points
forepaw drag 100,100 500,300 --app Finder --modifiers shift  # constrained drag
forepaw hover @e5 --app Finder      # move mouse to element (tooltip)
forepaw wait "Done" --app MyApp     # poll until text appears
forepaw batch --app Notes "click @e3 ;; keyboard-type hello ;; press return"
forepaw screenshot --app Finder     # take a screenshot
forepaw screenshot --app Finder --annotate  # numbered labels on elements
forepaw screenshot --app Finder --style spotlight --only @e1 @e3  # highlight specific refs
```

The agent loop: **observe -> decide -> act -> observe**

## Architecture

```
Sources/
  Forepaw/         # CLI (swift-argument-parser)
  ForepawCore/     # Platform-agnostic: protocol, ref system, tree rendering
  ForepawDarwin/   # macOS: AXUIElement, CGEvent, screencapture, Vision OCR
```

`ForepawCore` defines a `DesktopProvider` protocol. `ForepawDarwin` implements it for macOS. A future Linux implementation would use AT-SPI2/DBus with the same CLI interface.

Snapshots include screen coordinates for every element (`(x,y WxH)` format), making element positions visible for debugging and precise coordinate-based targeting.

## Requirements

- macOS 14+
- Swift 6.0+
- **Accessibility** permission (for snapshot, click, type)
- **Screen Recording** permission (for screenshot, ocr, ocr-click)

```bash
forepaw permissions          # check both permissions
forepaw permissions --request  # trigger system dialogs
```

## Setup

```bash
swift build
swift run forepaw permissions --request
swift run forepaw list-apps
```

## Commands

### Observation

| Command | Description |
|---------|-------------|
| `snapshot --app <name> [-i] [-c]` | Accessibility tree with `@e` refs |
| `screenshot [--app <name>] [--window <title\|id>] [--annotate\|--style <style>] [--only @eN...]` | Take a screenshot (PNG), optionally annotated |
| `ocr [--app <name>] [--window <title\|id>] [--find <text>]` | Screenshot + OCR, returns text with coordinates |
| `list-apps [--json]` | Running GUI applications |
| `list-windows [--app <name>]` | Visible windows with titles and IDs |

### Interaction

| Command | Description |
|---------|-------------|
| `click <@ref> --app <name> [--right] [--double]` | Click element (AX action first, mouse fallback) |
| `type <@ref> <text> --app <name> [--text <text>]` | Set element value / type into element |
| `ocr-click <text> --app <name> [--window <title\|id>] [--right] [--double] [--index N] [--text <text>]` | Find text via OCR and click it |
| `keyboard-type <text> [--app <name>] [--text <text>]` | Type into focused element |
| `press <combo> [--app <name>]` | Keyboard shortcut (e.g. `cmd+s`, `ctrl+shift+z`) |
| `drag <from> <to> [--app <name>] [--steps <n>] [--duration <s>] [--modifiers <keys>] [--pressure <0-1>] [--right] [--close] [--stdin]` | Drag between points (drawing, moving, resizing) |
| `scroll <direction> --app <name> [--window <title\|id>] [--amount <n>]` | Scroll up/down/left/right |
| `hover <@ref\|text> --app <name> [--window <title\|id>]` | Move mouse to element or text (triggers tooltips/hover states) |
| `wait <text> --app <name> [--timeout <s>] [--interval <s>] [--text <text>]` | Poll OCR until text appears |
| `batch <actions> [--app <name>] [--delay <ms>]` | Execute multiple actions (separated by `;;`) |

### Snapshot flags

| Flag | Description |
|------|-------------|
| `-i`, `--interactive` | Only interactive elements (buttons, fields, etc.) |
| `-c`, `--compact` | Remove empty structural nodes |
| `-d`, `--depth <n>` | Maximum tree depth (default: 15) |

## `--app` and `--window` behavior

- **With `--app`**: activates the app before acting. Required for `click`, `type`, `ocr-click`. Mouse clicks and keystrokes go to the right window.
- **Without `--app`**: sends input globally. Use for system-wide hotkeys (e.g. `press opt+space` for Raycast) or typing into whatever is currently focused.
- **With `--window`**: targets a specific window by title substring or window ID (from `list-windows`). Without it, commands target the largest window for the app.

```bash
forepaw list-windows --app Orion
# w-7290  Orion  "Hacker News"
# w-7291  Orion  "GitHub"

forepaw scroll down --app Orion --window "Hacker News"  # by title
forepaw screenshot --app Orion --window w-7290          # by ID
```

If a title substring matches multiple windows, forepaw reports the ambiguity and lists all matches with their IDs.

## Ref system

`snapshot` assigns `@e1`, `@e2`, `@e3` to interactive elements in depth-first order. Refs are positional -- action commands re-walk the tree to find the element, so refs work across CLI invocations as long as the UI hasn't changed. If a ref is stale, re-snapshot and retry.

`snapshot` activates the target app before reading the AX tree. This ensures refs match what action commands will see -- some apps (especially browsers) expose different elements when active vs. background.

The `--depth` flag controls how deep the AX tree is walked (default 15). Action commands like `click` also walk at depth 15, so refs are consistent at the default. Using a non-default `--depth` may cause ref mismatch with action commands.

Interactive roles: button, text field, text area, checkbox, radio button, slider, combo box, popup button, menu button, link, menu item, tab, switch, incrementor, color well, tree item, cell, dock item.

## OCR

For apps where the accessibility tree is empty or useless (Electron apps like Discord, Slack):

```bash
forepaw ocr --app Discord                    # all recognized text
forepaw ocr --app Discord --find "Bobby Tables"  # filter results
forepaw ocr-click "Bobby Tables" --app Discord   # find and click
```

OCR uses the macOS Vision framework (`VNRecognizeTextRequest`). No external dependencies. Coordinates are automatically adjusted for Retina displays and window position.

## Action strategies

**Click**: For elements found via `snapshot`, tries `AXPress` (accessibility action) first. For web content links in browsers, prefers mouse click (AXPress doesn't trigger navigation). Falls back to CGEvent mouse click at element center. `--right` for context menus, `--double` for double-click (file open, word select). Both flags always use mouse events (AXPress can't express these).

**Type**: Tries `AXSetAttributeValue` on the element's value first. Falls back to focusing the element via AX (`AXRaise` + `AXFocused`) and simulating keystrokes via CGEvent. More reliable than click + `keyboard-type` for text fields -- AX focus ensures the right element receives input. Inter-keystroke delay (8ms) prevents character dropping in Electron apps.

**Text starting with dashes**: `keyboard-type`, `type`, `ocr-click`, and `wait` accept `--text <value>` as an alternative to the positional text argument. Use it when the text starts with `-` or `--` to avoid it being parsed as a flag: `forepaw keyboard-type --text "--verbose" --app Notes`. The `--text` option unconditionally takes the next argument as its value.

**OCR-click**: Screenshots the window, runs OCR, finds the text, converts pixel coordinates to screen points (accounting for Retina scale factor and window offset), then clicks via CGEvent. When multiple matches are found, errors with a listing of all matches and their coordinates -- use `--index N` (1-based) to pick one. Single matches click without needing `--index`.

**Scroll**: Moves the mouse to the target position (window center by default, or element center with `--ref`), then fires CGEvent scroll wheel events. Amount is in "ticks" (lines), default 3.

**Drag**: Mouse drag with smooth interpolation between points. Supports two-point drag (`drag 100,100 500,500`), multi-point paths (`drag 100,100 300,200 500,100`), and ref-based drag (`drag @e3 @e7`). `--steps` controls smoothness per segment (default 30), `--duration` controls total time (default 0.3s). Uses CGEvent mouse drag events for intermediate points, which apps like Affinity, Figma, etc. recognize as continuous brush strokes or drag gestures. `--modifiers` holds keys during the drag (e.g. `--modifiers shift`, `--modifiers shift+alt`) -- Shift constrains to straight lines in drawing apps, Alt clones in design tools. `--close` appends the start point to close a multi-point path (triangles, polygons). `--right` uses right mouse button. `--pressure 0.0-1.0` sets tablet-style pressure (app must have pressure dynamics enabled). `--stdin` reads coordinates from stdin for complex paths with many points (e.g. `python3 -c "..." | forepaw drag --stdin --app App`).

**Hover**: Moves the mouse to the target without clicking. Accepts either an `@e` ref (from `snapshot`) or text (found via OCR). Triggers tooltips, hover states, dropdown previews.

**Wait**: Polls the screen via OCR (screenshot + text recognition) at a configurable interval until the target text appears or the timeout expires. Default 10s timeout, 1s interval.

**Batch**: Executes multiple actions sequentially in one process invocation. Actions are separated by `;;`. The `--app` and `--window` flags apply to all actions unless overridden per-action. Default 100ms delay between actions. Supported actions: click, drag, hover, type, keyboard-type, press, scroll, ocr-click, wait. **Use batch for any multi-step interaction** -- separate CLI invocations return control to the terminal between commands, which steals focus from the target app. Any click-then-type pattern needs batch.

**Annotated screenshots**: Captures a window screenshot, walks the AX tree for the same window, then overlays numbered labels on interactive elements using CoreGraphics. Labels use sequential display numbers (1, 2, 3...) with a legend mapping to `@e` refs. Three styles: `badges` (small colored pills -- agent-optimized), `labeled` (bounding boxes with role+name -- human-readable), `spotlight` (dims non-interactive areas). Color-coded by element category: green=buttons, yellow=text fields, blue=selection controls, purple=navigation. `--only @eN...` filters to specific refs. The annotation pipeline is split: `AnnotationCollector` (ForepawCore, platform-agnostic) walks the tree and collects annotation data, `AnnotationRenderer` (ForepawDarwin) draws on the image via CoreGraphics.

## Design decisions

- **Accessibility-first, not screenshot-first.** Text trees are ~50 lines vs ~1500 tokens for an image. OCR is the fallback, not the default.
- **AX actions before mouse simulation.** More reliable, doesn't move the physical cursor.
- **CLI, not library/daemon/MCP.** Works with any agent that can call shell commands.
- **Platform-agnostic core.** The ref system, tree rendering, and output formatting are in `ForepawCore` with no platform imports. Only `ForepawDarwin` touches macOS APIs.
- **App activation before input.** Mouse clicks and keystrokes target whatever window is under the cursor. Activating the app first ensures the right window receives input.
- **Built for agents, designed for humans too.** forepaw reads the same accessibility tree that screen readers use. Annotated screenshots bridge invisible structure to the visible -- useful for AI agents acting on apps with poor AX trees, but also for sighted people helping blind users debug UIs, low-vision users, or anyone trying to understand an app's interactive structure. The annotation system supports multiple styles for different audiences rather than optimizing solely for machine consumption.
