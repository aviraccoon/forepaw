# Changelog

## v0.2.0 "Coati" (2026-03-31)

### New commands

- `scroll` -- scroll up/down/left/right with configurable amount and element targeting

### New features

- `--window` flag for targeting specific windows by title substring or ID (`w-1234`)
  - Works with `screenshot`, `ocr`, `ocr-click`, `scroll`
  - `list-windows` now shows quoted titles and filters phantom windows
- `--right` flag on `click` and `ocr-click` for right-click (context menus)
- `--double` flag on `click` and `ocr-click` for double-click
- Word-level OCR targeting -- `ocr --find` and `ocr-click` now return bounding boxes for matched substrings, not entire text blocks

### Bug fixes

- **Fixed OCR click accuracy** -- screenshots no longer include window shadow padding (~34px offset on all sides). Adds `-o` flag to `screencapture`.
- **Fixed mouse click routing** -- CGEvent clicks now move the physical cursor before clicking, ensuring the click reaches the correct window.
- **Fixed phantom window selection** -- apps with tiny hidden windows (e.g. Orion's 1x1px "Preview" window) no longer break `screenshot`, `scroll`, or coordinate calculations. Selects largest window by area.

### Other

- Split `DarwinProvider.swift` (668 lines) into 4 focused files
- Split `Forepaw.swift` (425 lines) into 4 focused files
- `ClickOptions` and `MouseButton` types in ForepawCore (platform-agnostic)
- ClickTarget test app for visual click accuracy verification
- 32 unit tests (up from 22)

## v0.1.0 "Trash Panda" (2026-03-31)

Initial release.

### Commands

- `snapshot` -- accessibility tree with `@e` refs
- `click` -- click element by ref (AX action first, mouse fallback)
- `type` -- set element value / type into element
- `keyboard-type` -- type into focused element (no ref needed)
- `press` -- keyboard shortcut
- `screenshot` -- take a screenshot (PNG)
- `ocr` -- screenshot + Vision OCR, returns text with coordinates
- `ocr-click` -- find text via OCR and click it
- `list-apps` -- running GUI applications
- `list-windows` -- visible windows
- `permissions` -- check/request accessibility and screen recording permissions

### Architecture

- `ForepawCore` -- platform-agnostic protocol, ref system, tree rendering
- `ForepawDarwin` -- macOS implementation (AXUIElement, CGEvent, Vision OCR)
- Designed for future Linux support via AT-SPI2/DBus
