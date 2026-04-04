---
name: forepaw
description: Control macOS desktop apps for the user. Use when asked to interact with GUI applications, click buttons, fill forms, read screen content, or automate any desktop task. Read this before running any forepaw command.
---

# Desktop Automation with forepaw

`forepaw` is a CLI tool for controlling macOS applications. It works through bash -- every command goes through the permission gate.

## Core loop

**observe -> decide -> act -> observe**

Always snapshot or screenshot before acting. Never assume UI state from a previous command -- the UI may have changed.

## Observation (pick the right one)

### 1. Accessibility tree (prefer this)

```bash
forepaw snapshot --app "App Name" -i   # interactive elements only
```

Returns structured text with `@e` refs and screen positions:
```
app: Finder
window "Documents" (0,46 1200x800)
  button @e1 "Back" (10,50 60x30)
  textfield @e2 "Search" value="" (200,50 300x30)
  list (10,90 1180x700)
    cell @e3 "README.md" (10,90 1180x25)
    cell @e4 "src" (10,115 1180x25)
```

Every element includes `(x,y WxH)` screen coordinates. These match what action commands use -- you can verify click targets or use coordinate-based click/hover for precision.

Best for: native macOS apps (Finder, System Settings, Notes, Xcode, browsers' chrome). For browsers, the full tree (without `-i`) includes web content elements like links, headings, and text -- useful for clicking small targets like footnote links.

**Electron apps (Discord, Slack, VS Code, Cursor, Notion, Linear, etc.)** are automatically detected. forepaw sets `AXManualAccessibility` to tell Chromium to expose its web content tree. The first snapshot of an Electron app may take an extra 1-3s while the tree builds; subsequent snapshots are fast. No special flags needed -- just use `snapshot` as normal.

### 2. OCR (fallback for Electron apps)

```bash
forepaw ocr --app Discord                    # all text with coordinates
forepaw ocr --app Discord --find "Settings"  # filter
```

Returns text with click coordinates. Use when `snapshot` returns an empty or useless tree (Discord, Slack, VS Code, most Electron apps).

### 3. Screenshot (for visual inspection)

```bash
forepaw screenshot --app "App Name"   # plain screenshot
forepaw screenshot                    # full screen
```

Returns a PNG path. Use when you need to see what's on screen (debugging visual issues, checking layout). The image can be read with the `read` tool.

### 4. Annotated screenshot (visual + structural)

```bash
forepaw screenshot --app "App Name" --annotate           # numbered badges (default)
forepaw screenshot --app "App Name" --style badges        # same as --annotate
forepaw screenshot --app "App Name" --style labeled       # bounding boxes with role+name
forepaw screenshot --app "App Name" --style spotlight      # dims non-interactive areas
forepaw screenshot --app "App Name" --style spotlight --only @e5 @e8 @e12  # highlight specific refs
```

Overlays numbered labels on interactive elements. Each label maps to an `@e` ref. Prints a legend:
```
[1] @e1 Button "Save"
[2] @e3 TextField "Search"
[3] @e5 CheckBox "Enable"
```

Labels are color-coded by element type: green=buttons, yellow=text fields, blue=selection controls, purple=navigation.

**Styles:**
- `badges` -- small numbered pills. Minimal visual noise. Best for agents.
- `labeled` -- bounding boxes with role and name. Best for humans understanding UI structure.
- `spotlight` -- dims everything except interactive elements. Best for focusing attention.

**When to use:** When the AX tree is sparse (Electron apps) or you need visual context for spatial layout. Prefer `snapshot -i` for most tasks -- it's faster and cheaper in tokens. Use annotated screenshots when you need to correlate visual appearance with interactive elements.

## Actions

### Click by ref (from snapshot)

```bash
forepaw click @e3 --app "App Name"
forepaw click @e3 --app "App Name" --right    # right-click (context menu)
forepaw click @e3 --app "App Name" --double   # double-click
```

### Click by coordinates (from snapshot bounds)

```bash
forepaw click 500,300 --app "App Name"    # click at screen position
forepaw hover 500,300 --app "App Name"    # hover at screen position
```

Use when you have coordinates from snapshot bounds but no ref (e.g. static text, or when refs shift). Read the `(x,y WxH)` from snapshot output and compute the center: `x + W/2, y + H/2`.

### Click by text (from OCR)

```bash
forepaw ocr-click "Button Label" --app Discord
forepaw ocr-click "file.txt" --app Finder --double   # double-click
forepaw ocr-click "item" --app "App Name" --right    # right-click
```

`--right` and `--double` work on both `click` and `ocr-click`. Right-click opens context menus. Double-click for selecting words, opening files, etc.

When multiple matches are found, `ocr-click` errors with a listing:
```
Multiple matches for 'Shelter':
  --index 1: 'Shelter' at 608,138
  --index 2: 'Shelter' at 323,423
Use --index N to pick one.
```
Use `--index N` to click a specific match. Single matches click without needing `--index`. Prefer `click @ref` when available -- it's unambiguous.

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
forepaw drag 100,100 500,500 --app "App Name"                      # simple drag between two points
forepaw drag 100,100 300,200 500,100 700,300 --app "App Name"      # path through multiple waypoints
forepaw drag @e3 @e7 --app "App Name"                              # drag between two elements
forepaw drag 100,100 500,500 --app "App Name" --steps 60 --duration 1.0  # slower, smoother
forepaw drag 100,100 500,350 --app "App Name" --modifiers shift    # constrained (straight lines, 45-degree)
forepaw drag 100,100 500,500 --app "App Name" --modifiers shift+alt # combine modifiers
forepaw drag 100,100 300,200 500,100 --app "App Name" --close      # auto-close path back to start
forepaw drag 100,100 500,500 --app "App Name" --right              # right-button drag (panning, context menus)
forepaw drag 100,100 500,500 --app "App Name" --pressure 0.5       # tablet pressure simulation
```

Stdin mode for complex paths (circles, stars, curves -- generate coordinates programmatically):
```bash
echo "100,100 200,150 300,100 400,200" | forepaw drag --stdin --app "App Name"
python3 -c "import math; print(' '.join(f'{int(400+150*math.cos(i*2*math.pi/40))},{int(500+150*math.sin(i*2*math.pi/40))}' for i in range(41)))" | forepaw drag --stdin --app "App Name" --close --steps 20 --duration 2.0
```

Drags the mouse from one point to another with smooth interpolation. Supports coordinates, refs, or a mix. For paths with 3+ points, all must be coordinates.

- `--steps` controls smoothness per segment (default 30, higher = more intermediate points)
- `--duration` controls total drag time in seconds (default 0.3)
- Use higher steps and duration for drawing apps that need smooth curves
- `--modifiers shift+alt` holds modifier keys during the entire drag (supports shift, alt/opt, cmd, ctrl, combinable with `+`)
- `--close` appends start point to end of path, closing the shape (3+ points only)
- `--right` uses right mouse button instead of left
- `--pressure 0.0-1.0` sets mouse pressure (apps must have pressure dynamics enabled)
- `--stdin` reads coordinates from stdin (space or newline separated x,y pairs) -- use for complex paths with many points
- Works in batch: `forepaw batch --app App "drag 100,100 500,500 --modifiers shift"`

### Scroll

```bash
forepaw scroll down --app Orion              # scroll down 3 ticks (default)
forepaw scroll up --app Orion --amount 10    # scroll up 10 ticks
forepaw scroll left --app Finder             # horizontal scroll
forepaw scroll down --app Orion --ref @e5    # scroll within a specific element
```

Directions: `up`, `down`, `left`, `right`. Default amount is 3 ticks.

### Hover (trigger tooltips/hover states)

```bash
forepaw hover @e5 --app "App Name"              # by ref (from snapshot)
forepaw hover "Submit" --app "App Name"          # by text (OCR lookup)
forepaw hover "8 comments" --app Orion           # hover over a link
```

Moves the mouse without clicking. Accepts either an `@e` ref or text (auto-detected -- if the argument parses as a ref, uses AX; otherwise uses OCR). Useful for triggering tooltips, hover menus, or preview popups.

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

Supported actions: `click`, `hover`, `type`, `keyboard-type`, `press`, `scroll`, `ocr-click`, `wait`.

Per-action overrides:
```bash
forepaw batch "press opt+space ;; keyboard-type --app Raycast search term"
```

**Use batch for any multi-step interaction.** Separate CLI invocations return control to the terminal between commands, which steals focus from the target app. Batch keeps the app focused throughout the entire sequence. This is essential for workflows like typing into a text field after clicking it, or any click-then-type pattern.

Browser URL bar example:
```bash
forepaw batch --app Orion "click 626,72 ;; keyboard-type example.com ;; press return"
```

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

forepaw screenshot --app Zed --window "my-project"   # by title substring
forepaw screenshot --app Zed --window w-1234         # by window ID
forepaw scroll down --app Zed --window "my-project"  # works with scroll too
forepaw ocr --app Zed --window "my-project"           # and OCR
forepaw ocr-click "text" --app Zed --window "my-project"  # and ocr-click
```

Without `--window`, commands target the largest window for that app.

The title shown in quotes in `list-windows` output is what you pass to `--window`. If the title matches multiple windows, forepaw returns an error listing all matches with their IDs.

## When to use --app

- **With --app**: activates the app before acting. Use for click, type, keyboard-type, press when targeting a specific app.
- **Without --app**: sends input globally. Use for system hotkeys (Raycast, Spotlight) or typing into whatever is already focused.

## Important behaviors

- **Always observe before acting.** Don't guess UI state.
- **Refs are positional.** `@e3` means "the 3rd interactive element in depth-first order." If the UI changes (menu opens, dialog appears), refs shift. Re-snapshot after any action that changes the UI. Don't use `--depth` with a non-default value and expect refs to work with action commands -- `--depth` controls the tree walk, and action commands use the default depth (15).
- **Snapshot activates the app.** The snapshot command brings the app to the foreground so the AX tree matches what action commands will see. Some apps (especially browsers) expose different elements when active vs. background.
- **Prefer `type @ref` over click + keyboard-type.** `type` focuses the element via AX and types into it directly. `keyboard-type` after a click can fail if the click didn't give the element AX focus. Use `keyboard-type` only inside batch (after coordinate clicks) or when no ref is available.
- **Use batch for multi-step interactions.** Separate CLI invocations return control to the terminal, which steals focus from the target app. Any click-then-type or multi-action sequence should use batch. Even adding `--delay` for slow UI transitions.
- **AX tree vs OCR.** Try `snapshot -i` first. Electron apps are auto-detected and their web content trees are enabled automatically. If the tree is still sparse after this, fall back to OCR.
- **App activation.** `--app` brings the app to the foreground. This means the user's screen will change. Warn them before switching apps if they didn't explicitly ask.
- **Mouse clicks are physical.** OCR-click and mouse-fallback clicks move the actual cursor and click on screen. The user will see this happening.
- **Coordinate clicks are bounds-checked.** When `--app` is specified, `click` and `hover` with coordinates will error if the point is outside the target window. This prevents destructive misclicks on other apps. If you get a bounds error, re-snapshot -- the window may have moved.
- **Keystroke delay.** Typing is not instant (~8ms per character). Long text takes a moment.
- **Wait timeout.** `wait` polls via OCR (screenshot + text recognition each poll). Keep intervals reasonable (1s+) to avoid hammering the system. The default 10s timeout covers most UI transitions.
- **Text starting with dashes.** If text for `keyboard-type`, `type`, `ocr-click`, or `wait` starts with `-` or `--`, use the `--text` option instead of a positional argument:
  ```bash
  forepaw keyboard-type --text "--this starts with dashes" --app Notes
  forepaw type @e5 --text "-dash text" --app Notes
  forepaw ocr-click --text "--Settings" --app App
  ```
  `--text` unconditionally takes the next argument as its value, even if it looks like a flag.

## Permissions

If commands fail with permission errors:
```bash
forepaw permissions          # check status
forepaw permissions --request  # trigger system dialogs
```

Two permissions needed:
- **Accessibility** (System Settings > Privacy & Security > Accessibility) -- for snapshot, click, type
- **Screen Recording** (System Settings > Privacy & Security > Screen & System Audio Recording) -- for screenshot, ocr, ocr-click

## Discovering apps and windows

```bash
forepaw list-apps                  # running GUI apps with bundle IDs
forepaw list-windows --app Finder  # windows for an app
```

Use the exact app name from `list-apps` in `--app` flags. Use `list-windows` to find window titles/IDs for `--window`.
