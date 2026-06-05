# Changelog

## v0.4.0 "The Trash Crab" (2026-06-05)

The cross-platform release. forepaw was rewritten from Swift to Rust, adding Windows and Linux support alongside the existing macOS backend. New observation commands (`hit-test`), richer element output (state, identifiers, signatures), global `--format json` and `--verbose` flags, `FOREPAW_LOG` for debugging, and a faster Electron snapshot path by fixing a polling bug that added a 3-second delay to every Electron snapshot.

### New commands

- `hit-test <x,y>` — find what element is at screen coordinates, with ancestor chain, role, name, value, available actions, and PID. Useful for inspecting unnamed elements and understanding element relationships.

### Observation features

- **Element state in snapshots** — `disabled`, `focused`, and `selected` now appear inline in tree output where the platform provides them. Currently populated on macOS. (macOS)
- **Window state in list-windows** — fullscreen windows are now identified via `state: fullscreen`. The `state` field on `WindowInfo` supports Normal, Minimized, Maximized, and Fullscreen for future expansion. (macOS)
- **`is_active` in list-apps** — each app entry shows whether it's the frontmost (active) application. (macOS)
- **`description` field** — element descriptions (AXDescription on macOS) stored separately and shown in verbose output, rather than consumed silently by the name resolution fallback.
- **`native_role` field** — the platform's original role string (e.g. `"AXButton"`, `"UIA 50000"`) preserved for debugging. Shown in verbose mode.
- **`identifier` field** — stable cross-launch element identifier (AXIdentifier on macOS), where available. Shown in verbose mode.
- **Verbose tree rendering** (`--verbose`) — shows `description`, `native_role`, `identifier`, `uid`, and `signature` alongside each element. Default output stays clean.
- **Lowercase role text** — snapshot output now shows `button`, `textfield`, `window` instead of `AXButton`, `AXTextField`, `AXWindow`. Cleaner, platform-agnostic.
- **UID and signature fields** — every element gets a depth-first sequence number (`uid`) and a deterministic content hash (`signature`) for cross-snapshot identity matching. Visible in verbose mode.
- **Typed Role enum** — all element roles are now typed (`role: Role`) instead of raw platform strings. The platform's original role is preserved as `native_role` for debugging.

### CLI improvements

- **`--pid <N>`** — target applications by process ID. Mutually exclusive with `--app`.
- **`--window-id <ID>`** — target windows by numeric ID from `list-windows`. Accepts bare IDs (7290) and w-prefixed (w-1234). Mutually exclusive with `--window`.
- **`--format json`** — global output format flag. Every command supports it consistently (replaced Swift's per-command `--json` flag which was inconsistently available). Snapshot JSON serializes the full element tree with typed fields.
- **`--verbose`** — global flag for richer output across all observation commands.
- **`FOREPAW_LOG` env var** — zero-dependency structured logging. `FOREPAW_LOG=snapshot=debug` enables per-module filtering. Falls back to `RUST_LOG`.
- **`--version` with git SHA** — binaries now show the commit SHA they were built from (e.g. `forepaw 0.4.0 (abc1234)`), built at compile time rather than resolved at runtime. Nix builds use the flake's source hash.

### Platform support (new)

- **Windows** — `list-apps`, `list-windows`, `snapshot` (UIA ControlView tree), `screenshot` (per-window via PrintWindow with PW_RENDERFULLCONTENT, full-screen via BitBlt), `OCR` (Windows.Media.Ocr with 3× Lanczos upscale for small text), and `hit-test` (UIA ElementFromPoint). UWP apps (Calculator, Settings, Photos) are detected correctly through their ApplicationFrameHost process. Action commands (click, type, press, scroll, drag) are stubbed with clear error messages.
- **Linux** — `list-apps`, `list-windows`, `snapshot` (AT-SPI2 tree walk via D-Bus with generated role mapping covering 131 AT-SPI2 roles), and `hit-test` (Component.GetAccessibleAtPoint). Works with both Qt (KDE) and GTK (GNOME) apps. Screenshot, OCR, and all action commands are not yet implemented.
- **macOS backend rewritten** — Same capabilities as Swift v0.3.0, plus: `hit-test` command, element state (enabled/focused/selected), window state detection, `identifier` field, `description` as a separate field, stale AX child retry for lazy initialization (e.g. Slint), additional validation for macOS screen recording permission (catches TCC new-binary redaction), and batched Electron tree population check (fixes 3s polling delay).

### Architecture

- **Swift → Rust rewrite** — the entire codebase was rewritten from Swift to Rust. Workspace split into two crates: `forepaw` (library, minor-range dep pins for downstream consumers) and `forepaw-cli` (binary, exact dep pins for supply chain control). Clean platform abstraction via `DesktopProvider` trait with cfg-gated backends.
- **Cross-platform role mapping** — typed `Role` enum with 57 variants replaces Swift's raw string roles. Each platform backend maps its native role type (AXRole string, UIA ControlType ID, AT-SPI2 role number) to the shared enum. Unknown roles map to `Role::Unknown`.
- **ElementData / ElementNode split** — element properties separated from tree structure, enabling flat data access without recursive traversal. `uid` and `signature` fields for cross-snapshot element identity via FNV-1a content hashing.
- **Tree pruning module** — pruning logic extracted from macOS backend into a cross-platform `PruningOptions` struct, shared across all three platforms.
- **Build-time git SHA** — `build.rs` embeds the commit SHA at compile time with Nix reproducibility support and dirty-tree detection.

### Bug fixes

- **Electron polling timeout** — the Electron tree population check always timed out (3 seconds) because individual `AXUIElementCopyAttributeValue` calls return errors on partially-built Electron accessibility trees. Now uses the batched attribute fetch path — first poll returns in ~500ms instead of 3 seconds. This affected Swift v0.3.0 as well.
- **Stale AX child references** — apps with lazy accessibility initialization (e.g. Slint) could return invalid `AXUIElement` references from `AXChildren`. The fix detects stale refs (elements returning `Role::Unknown` with no name/value/bounds) and re-reads `AXChildren` from the parent to get fresh references. Genuinely broken elements still show as diagnostic `unknown` nodes.

### Other

- Cross-platform CI/CD: GitHub Actions now runs on macOS (native build + cross-arch check), Windows (native build on windows-2025, both aarch64 + x86_64), and Linux (Nix-based musl build). All actions pinned to commit SHAs for supply chain defense.
- Release workflow produces 10 artifacts across 5 platform/arch combinations (macOS arm64, Windows x86_64 + arm64, Linux x86_64 + arm64 musl-static), each with SHA256 checksums and platform-specific install instructions.
- `forepaw` library crate published on [crates.io](https://crates.io/crates/forepaw) — subsequent releases publish automatically via Trusted Publishing (OIDC). No API tokens to manage.
- Nix flake: package definition for all 4 nixpkgs systems, dev shell with Rust + cross-compilation tooling (cargo-xwin for Windows, cargo-zigbuild for Linux musl).
- Strict Rust lint configuration: `clippy::pedantic` at deny, cherry-picked restriction lints at warn (no panic, unsafe discipline, tripwires). Per-site `#[expect]` for cast lints.
- Dependency auditing via `cargo audit` and `cargo machete`.
- `mise.toml` expanded with cross-compilation tasks (`build-windows`, `build-linux`, `lint-all` with 6 platform targets).

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
