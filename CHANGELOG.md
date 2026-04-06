# Changelog

## v0.3.0 "Kinkajou" (2026-04-06)

The "reach into everything" release. Electron apps, CEF apps, region targeting, snapshot diffing, annotated screenshots, drag, batch actions, and a 100x performance improvement on Apple Music.

### New commands

- `hover` -- move mouse to ref, text, or coordinates (triggers tooltips, hover states)
- `wait` -- poll OCR until text appears (configurable timeout/interval)
- `batch` -- multiple actions in one invocation (`;;` separator), keeps app focus throughout
- `drag` -- drag between points or along paths, with modifiers, pressure, stdin input, right-button, `--close` for shapes

### Observation features

- **Snapshot diffing** (`--diff`) -- compare before/after snapshots to see what changed. Ref-shift-aware: positional renumbering doesn't produce false changes. Auto-cached per app.
- **Annotated screenshots** -- three styles: `badges` (numbered pills), `labeled` (bounding boxes with role+name), `spotlight` (dims non-interactive areas). Color-coded by element type.
- **Area screenshots** (`--ref @eN`, `--region x,y,w,h`) -- crop screenshots to specific elements or regions with padding. Annotations render before cropping.
- **Grid overlay** (`--grid N`) -- coordinate grid lines every N pixels on screenshots. Useful for human debugging.
- **OCR returns screenshots** -- `ocr` now saves an agent-friendly display copy alongside text results. `--no-screenshot` to skip.
- **Snapshot timing** (`--timing`) -- adaptive per-subtree breakdown showing where time is spent.

### Action features

- **Region click** (`click x,y,w,h`) -- find and click the most visually prominent element in a rough bounding box. Uses pixel saturation centroid. Solves icon targeting in CEF apps.
- **Region hover** (`hover x,y,w,h`) -- same saliency detection, moves cursor instead of clicking.
- **Smooth hover** (`--smooth`) -- interpolated mouse movement for apps that track mouseEnter/mouseLeave.
- **Coordinate scroll** (`scroll --at x,y`) -- target specific panels without refs.
- **Coordinate validation** -- click, hover, scroll, and drag reject out-of-bounds coordinates with an error (not a clamp).
- `--text` option on `type`, `keyboard-type`, `ocr-click`, `wait` for dash-prefixed text arguments.
- `meta`/`super` accepted as aliases for `cmd` in key combos.

### Electron & CEF

- **Auto-detect Electron apps** and enable accessibility via `AXManualAccessibility`. Discord, Slack, VS Code, Cursor, Obsidian, Notion, Linear -- no flags needed.
- **Icon class parsing** -- extract semantic names from CSS classes (Lucide, Tabler, FontAwesome, Material, Heroicons, Phosphor, Bootstrap, Feather, Ionicons, Octicons, Codicons). An unnamed button with `lucide-settings` becomes `button "settings"`.
- **CEF apps** (Spotify, Steam) detected separately -- they don't respond to AXManualAccessibility. OCR, screenshots, and region targeting work.
- **Multi-process app discovery** -- apps like Steam render UI in a helper process. forepaw finds these windows automatically via bundle ID prefix matching.

### Performance

- **Batched attribute fetch** -- 13 attributes per element in one IPC call (was 7-8 + individual fallbacks). Music: ~50s to ~12s.
- **Offscreen pruning** (default-on) -- skip subtrees outside window bounds. Music: ~30s to 130ms (200+ invisible play history rows skipped).
- **Children-first name resolution** -- build child nodes before computing parent names. Reads from in-memory objects instead of making per-child IPC calls.
- **Single-pass tree walk** -- build tree and collect AXUIElement handles in one walk instead of two.
- **Smart defaults** (`-i` mode) -- auto-skip menu bar (200-300 elements) and zero-size elements.
- **Screenshot optimization** -- auto-detect WebP (via cwebp), default to 1x JPEG. 4-17x smaller files (630KB-3.3MB down to 85-150KB).

### Architecture

- **Window-relative coordinates** everywhere. `(0,0)` is window top-left. Coordinates don't break when the window moves.
- **DesktopProvider protocol enforcement** -- CLI no longer imports ForepawDarwin directly. If a method isn't on the protocol, it doesn't compile.
- **Attribute mining** for better element names: AXTitleUIElement, first child scan, AXHelp, AXPlaceholderValue, AXDOMClassList (icon classes), AXRoleDescription. Unnamed elements reduced 20-58% across tested apps.

### Documentation

- README rewritten for public audience (motivation, quick start, command table, feature highlights)
- `docs/internals.md` rewritten with current architecture (with raccoons)
- `docs/performance-macos.md` -- benchmark data across 12 apps
- `docs/cross-platform.md` -- Linux (AT-SPI2/KDE/GNOME) and Windows (UIA) feasibility research
- Skill file expanded with CEF workflow, region targeting, batch patterns

### Bug fixes

- **Fixed version string in installed binaries** -- `--version` was running `git describe` at runtime, producing garbage outside the source repo. Now uses a compile-time `RELEASE_BUILD` flag; release binaries report clean version numbers.

### Other

- CI: mise tasks, SPM build cache, Node 24 action bumps, Package.resolved tracked
- Release workflow includes codename in GitHub Release title
- 146 unit tests (up from 32)

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
