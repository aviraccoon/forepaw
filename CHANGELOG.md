# Changelog

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
