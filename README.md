# forepaw

Desktop automation CLI for AI agents. Control any application through OS accessibility trees, OCR, and input simulation.

Named after the raccoon's dexterous forepaws -- precise manipulation of UI elements without brute force.

## How it works

Three observation strategies, used based on what the target app exposes:

1. **Accessibility tree** (best) -- structured text with `@e` refs. Works well for native macOS apps.
2. **OCR** -- screenshot + Vision framework text recognition. Fallback for Electron apps (Discord, Slack) with poor accessibility.
3. **Screenshots** -- visual fallback for anything else. Can be sent to vision models.

```
forepaw snapshot --app Finder -i    # accessibility tree with refs
forepaw click @e3 --app Finder      # click element by ref
forepaw ocr --app Discord           # OCR the window, get text + coordinates
forepaw ocr-click "Settings" --app Discord  # find text and click it
forepaw keyboard-type "hello" --app Notes   # type into focused element
forepaw press cmd+s --app Finder    # keyboard shortcut
forepaw press opt+space             # global hotkey (no --app)
forepaw scroll down --app Finder    # scroll down 3 ticks
forepaw screenshot --app Finder     # take a screenshot
forepaw screenshot --app Zed --window "my-project"  # target specific window
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
| `screenshot [--app <name>] [--window <title\|id>]` | Take a screenshot (PNG) |
| `ocr [--app <name>] [--window <title\|id>] [--find <text>]` | Screenshot + OCR, returns text with coordinates |
| `list-apps [--json]` | Running GUI applications |
| `list-windows [--app <name>]` | Visible windows with titles and IDs |

### Interaction

| Command | Description |
|---------|-------------|
| `click <@ref> --app <name> [--right] [--double]` | Click element (AX action first, mouse fallback) |
| `type <@ref> <text> --app <name>` | Set element value / type into element |
| `ocr-click <text> --app <name> [--window <title\|id>] [--right] [--double]` | Find text via OCR and click it |
| `keyboard-type <text> [--app <name>]` | Type into focused element |
| `press <combo> [--app <name>]` | Keyboard shortcut (e.g. `cmd+s`, `ctrl+shift+z`) |
| `scroll <direction> --app <name> [--window <title\|id>] [--amount <n>]` | Scroll up/down/left/right |

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

**Type**: Tries `AXSetAttributeValue` on the element's value first. Falls back to focusing the element and simulating keystrokes via CGEvent. Inter-keystroke delay (8ms) prevents character dropping in Electron apps.

**OCR-click**: Screenshots the window, runs OCR, finds the text, converts pixel coordinates to screen points (accounting for Retina scale factor and window offset), then clicks via CGEvent.

**Scroll**: Moves the mouse to the target position (window center by default, or element center with `--ref`), then fires CGEvent scroll wheel events. Amount is in "ticks" (lines), default 3.

## Design decisions

- **Accessibility-first, not screenshot-first.** Text trees are ~50 lines vs ~1500 tokens for an image. OCR is the fallback, not the default.
- **AX actions before mouse simulation.** More reliable, doesn't move the physical cursor.
- **CLI, not library/daemon/MCP.** Works with any agent that can call shell commands.
- **Platform-agnostic core.** The ref system, tree rendering, and output formatting are in `ForepawCore` with no platform imports. Only `ForepawDarwin` touches macOS APIs.
- **App activation before input.** Mouse clicks and keystrokes target whatever window is under the cursor. Activating the app first ensures the right window receives input.
