---
name: forepaw
description: Control macOS desktop apps for the user. Use when asked to interact with GUI applications, click buttons, fill forms, read screen content, or debug visual issues in native/Electron apps.
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

Returns structured text with `@e` refs:
```
app: Finder
window "Documents"
  button @e1 "Back"
  textfield @e2 "Search" value=""
  list
    cell @e3 "README.md"
    cell @e4 "src"
```

Best for: native macOS apps (Finder, System Settings, Notes, Xcode, browsers' chrome).

### 2. OCR (fallback for Electron apps)

```bash
forepaw ocr --app Discord                    # all text with coordinates
forepaw ocr --app Discord --find "Settings"  # filter
```

Returns text with click coordinates. Use when `snapshot` returns an empty or useless tree (Discord, Slack, VS Code, most Electron apps).

### 3. Screenshot (for visual inspection)

```bash
forepaw screenshot --app "App Name"   # app window
forepaw screenshot                    # full screen
```

Returns a PNG path. Use when you need to see what's on screen (debugging visual issues, checking layout). The image can be read with the `read` tool.

## Actions

### Click by ref (from snapshot)

```bash
forepaw click @e3 --app "App Name"
forepaw click @e3 --app "App Name" --right    # right-click (context menu)
forepaw click @e3 --app "App Name" --double   # double-click
```

### Click by text (from OCR)

```bash
forepaw ocr-click "Button Label" --app Discord
forepaw ocr-click "file.txt" --app Finder --double   # double-click
forepaw ocr-click "item" --app "App Name" --right    # right-click
```

`--right` and `--double` work on both `click` and `ocr-click`. Right-click opens context menus. Double-click for selecting words, opening files, etc.

### Type into element (from snapshot)

```bash
forepaw type @e2 "search query" --app "App Name"
```

### Type into current focus (no ref needed)

```bash
forepaw keyboard-type "hello world" --app "App Name"  # activates app first
forepaw keyboard-type "hello world"                     # types into current focus
```

### Keyboard shortcuts

```bash
forepaw press cmd+s --app "App Name"   # activates app first
forepaw press opt+space                 # global hotkey (no --app)
```

### Scroll

```bash
forepaw scroll down --app Orion              # scroll down 3 ticks (default)
forepaw scroll up --app Orion --amount 10    # scroll up 10 ticks
forepaw scroll left --app Finder             # horizontal scroll
forepaw scroll down --app Orion --ref @e5    # scroll within a specific element
```

Directions: `up`, `down`, `left`, `right`. Default amount is 3 ticks.

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
- **Refs are positional.** `@e3` means "the 3rd interactive element in depth-first order." If the UI changes (menu opens, dialog appears), refs shift. Re-snapshot after any action that changes the UI.
- **AX tree vs OCR.** Try `snapshot -i` first. If the tree is mostly empty (just window buttons and menu bar), the app is Electron -- switch to OCR.
- **App activation.** `--app` brings the app to the foreground. This means the user's screen will change. Warn them before switching apps if they didn't explicitly ask.
- **Mouse clicks are physical.** OCR-click and mouse-fallback clicks move the actual cursor and click on screen. The user will see this happening.
- **Keystroke delay.** Typing is not instant (~8ms per character). Long text takes a moment.

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
