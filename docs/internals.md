# Internals

How forepaw works under the hood, and known limitations.

## macOS Accessibility API

The core observation mechanism. `AXUIElementCreateApplication(pid)` gives a handle to an app's UI hierarchy. From there:

- `AXUIElementCopyAttributeValue` reads attributes (`AXRole`, `AXTitle`, `AXValue`, `AXChildren`, `AXPosition`, `AXSize`)
- `AXUIElementPerformAction` performs actions (`AXPress`, `AXRaise`, `AXSetValue`)
- `AXUIElementSetAttributeValue` sets values directly (`AXValue`, `AXFocused`)

The tree is walked depth-first with a configurable max depth (default 15). Each interactive element gets a positional ref (`@e1`, `@e2`, ...) based on its depth-first order.

### Element name resolution

Many elements don't expose a direct `AXTitle`. The resolution chain:

1. `AXTitle` (explicit title)
2. `AXDescription` (accessibility description)
3. `AXTitleUIElement` -> read that element's `AXValue` or `AXTitle` (label pointer)
4. First `AXStaticText` child's `AXValue` (common in cells, rows)

This covers Finder sidebar items, table cells, and similar containers where the label is a child element.

### Ref resolution across invocations

Each CLI invocation creates a fresh `DarwinProvider`. Refs from `snapshot` are just positional numbers. When `click @e10 --app Finder` runs, it re-walks the tree counting interactive elements until it hits the 10th one. This works as long as the UI hasn't changed between snapshot and action.

If the UI did change (menu opened, dialog appeared), the ref is stale and may point to a different element. The fix is always: re-snapshot, get new refs, retry.

### Action dispatch

**Click**: For most roles, tries `AXPress` first (the accessibility action). For `AXLink` elements, uses mouse click directly -- browsers don't navigate on `AXPress` for web content links. Falls back to CGEvent mouse click at the element's center coordinates.

**App activation**: Before any mouse click or keystroke targeting an app, `NSRunningApplication.activate()` is called with a 300ms delay. CGEvent posts to whatever window is under the cursor, so the target app must be frontmost. Without activation, clicks go to the wrong window.

## OCR (Vision framework)

For apps where the accessibility tree is empty (Electron apps), forepaw screenshots the window and runs `VNRecognizeTextRequest`.

### Coordinate mapping

Vision returns normalized coordinates (0-1) with origin at bottom-left. These need three transformations:

1. **Denormalize**: multiply by image pixel dimensions
2. **Flip Y axis**: Vision origin is bottom-left, screen origin is top-left
3. **Retina scaling**: divide pixel coordinates by `NSScreen.backingScaleFactor` (typically 2.0)
4. **Window offset**: add the window's screen-space origin (`CGWindowListCopyWindowInfo`)

Steps 3-4 happen in `ocrClick`. The `ocr` command returns image-space coordinates (steps 1-2 only).

### OCR settings

- Recognition level: `.accurate` (not `.fast`) for reliability
- Language correction: disabled -- preserves usernames, IDs, and technical text that autocorrect would mangle

## Input simulation

### Keyboard (CGEvent)

Keystrokes are synthesized via `CGEvent(keyboardEventSource:virtualKey:keyDown:)`. For text input, each character is sent as a unicode string on a virtual key event. Named keys (return, escape, arrows, function keys) map to virtual key codes via `KeyCodeMap`.

Modifier keys (cmd, shift, opt, ctrl) are set via `CGEventFlags` on the key event.

**Inter-character delay**: 8ms between keystrokes. Electron apps (Discord, Slack, VS Code) have async input handling that drops characters if events arrive too fast. Native macOS apps handle any speed fine, but the delay is always applied for consistency.

### Mouse (CGEvent)

Mouse clicks are `leftMouseDown` + `leftMouseUp` at the target coordinates, posted to `.cghidEventTap`. No move event is synthesized first -- the click teleports to the coordinates.

## Permissions

Two separate macOS permissions are required:

| Permission | Used by | API check |
|-----------|---------|-----------|
| Accessibility | snapshot, click, type | `AXIsProcessTrusted()` |
| Screen Recording | screenshot, ocr, ocr-click | `CGPreflightScreenCaptureAccess()` |

Both are per-app (granted to the terminal, not to forepaw itself). After granting, the terminal may need a restart for the permission to take effect.

## Platform abstraction

`ForepawCore` defines `DesktopProvider`, a protocol with no platform-specific types. The macOS implementation (`ForepawDarwin/DarwinProvider`) uses `AXUIElement` handles stored in an in-memory ref table.

A Linux implementation would use AT-SPI2 over DBus for the accessibility tree and XDotool/libxdo or uinput for input simulation. The CLI, ref system, tree rendering, and output formatting would be identical.

## Known limitations

- **Electron apps**: Accessibility trees are mostly empty. OCR works but is slower and less precise than tree-based interaction.
- **Retina coordinate math**: OCR coordinates assume the primary display's scale factor. Multi-display setups with different scale factors may produce incorrect click positions.
- **Menu timing**: Opening a dropdown menu changes the accessibility tree. A snapshot taken immediately after clicking a menu button may not include the menu items if the UI hasn't updated yet. Add a small delay between click and snapshot.
- **Web content in browsers**: AXPress on links doesn't trigger navigation in some browsers (confirmed in Orion). Mouse click works but requires app activation.
- **Arguments starting with `--`**: ArgumentParser interprets `--` as end-of-options. Text arguments starting with dashes need `--app` placed before the positional argument.
- **Window-specific screenshots**: Uses `screencapture -l <windowID>` which captures the window as-is, including any overlapping windows. Not a clean window capture if other windows are on top.
