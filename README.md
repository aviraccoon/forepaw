# forepaw

A raccoon's paws on your desktop. Cross-platform automation CLI. Control any application through accessibility trees, OCR, and input simulation.

Named after the raccoon's dexterous forepaws -- precise manipulation of UI elements without brute force.

## What is this?

forepaw lets programs (and people, through programs) interact with any desktop application the same way a human would: reading what's on screen, clicking buttons, typing text, scrolling around. On macOS it reads the same accessibility tree that VoiceOver uses. On Windows it uses UI Automation (the same tree Narrator uses). Input simulation works through native platform APIs on both.

The original motivation was curiosity -- what would it take to let an AI agent use a desktop app? But the interesting part turned out to be bigger than that. An LLM with forepaw can operate applications on behalf of anyone: navigating complex UIs, filling out forms, reading screen content aloud, automating repetitive tasks. For blind and low-vision users, this means an AI assistant that can see and describe what's on screen, click the right buttons, and read back results -- using the same accessibility infrastructure that was always there, just with a more capable intermediary.

forepaw is the paws. The brain is whatever you connect them to.

## Quick start

```bash
# Build (requires Rust / cargo)
cargo build

# Grant permissions (Accessibility + Screen Recording)
cargo run -- permissions --request

# See what's running
cargo run -- list-apps
```

### The core loop: observe, act, observe

```bash
# 1. Look at what's on screen
forepaw snapshot --app Finder -i
```
```
app: Finder
window "Recents" (0,0 1024x678)
  button @e1 "Back" (7,4 28x24)
  button @e2 "Forward" (39,4 28x24)
  ...
  cell @e14 "README.md" (338,196 625x24)
  cell @e15 "Package.swift" (338,220 625x24)
```

```bash
# 2. Act on what you see
forepaw click @e14 --app Finder

# 3. See what changed
forepaw snapshot --app Finder -i --diff
```
```
[diff: 2 added, 1 removed, 18 unchanged]

- cell @e14 "README.md" (338,196 625x24)
+ cell @e14 "README.md" selected (338,196 625x24)
+ button @e20 "Quick Look" (892,4 40x24)
```

That's it. Snapshot gives you refs (`@e1`, `@e2`, ...), you use those refs to act, then snapshot again to see the result. Every ref is a handle to a real UI element -- a button, a text field, a menu item.

## What it can do

| | Command | What happens |
|-|---------|-------------|
| **See** | `snapshot --app Finder -i` | Read the accessibility tree with `@e` refs |
| | `screenshot --app Finder` | Take a screenshot (WebP/JPEG, 1x) |
| | `screenshot --app Finder --annotate` | Screenshot with numbered labels on elements |
| | `ocr --app Discord` | Screenshot + text recognition with coordinates |
| **Click** | `click @e3 --app Finder` | Click an element (AX action, mouse fallback) |
| | `click @e3 --app Finder --right` | Right-click (context menu) |
| | `click 500,300 --app Finder` | Click at window-relative coordinates |
| | `click 310,420,80,70 --app Spotify` | Find & click prominent element in a region |
| | `ocr-click "Settings" --app Discord` | Find text on screen and click it |
| **Type** | `type @e2 "hello" --app Notes` | Focus element and type into it |
| | `keyboard-type "hello" --app Notes` | Type into whatever is focused |
| | `press cmd+s --app Finder` | Keyboard shortcut |
| | `press opt+space` | Global hotkey (no `--app`) |
| **Navigate** | `scroll down --app Orion` | Scroll (up/down/left/right) |
| | `scroll down --app Discord --at 36,400` | Scroll a specific panel by coordinates |
| | `hover @e5 --app Finder` | Move mouse to element (tooltips, hover states) |
| | `drag 100,100 500,500 --app Figma` | Drag between points |
| **Compose** | `batch --app Notes "click @e3 ;; keyboard-type hello ;; press return"` | Multiple actions in one invocation |
| | `wait "Upload complete" --app App` | Poll until text appears on screen |

All coordinates are **window-relative** -- `(0,0)` is the top-left of the window, not the screen. Coordinates don't change when the window moves. Out-of-bounds coordinates are rejected (a misplaced click on the wrong app could be destructive).

## Electron apps just work

Some trash cans have a hidden compartment. Discord, Slack, VS Code, Cursor, Obsidian, Notion, Linear -- these Electron apps have a full accessibility tree inside, but they don't expose it unless asked. forepaw detects them automatically and flips the switch (via `AXManualAccessibility`, the same signal VoiceOver sends). No flags needed.

Electron apps with icon libraries (Lucide, Tabler, FontAwesome, etc.) get automatic icon name resolution from CSS classes. An unnamed button with a `lucide-settings` class becomes `button @e5 "settings"`.

For the rare Electron app where the tree is still sparse, `ocr` and `ocr-click` fill the gaps with Vision framework text recognition.

## CEF apps (Spotify, Steam)

Some apps use Chromium Embedded Framework instead of Electron. CEF doesn't respond to `AXManualAccessibility`, so the accessibility tree is empty. forepaw operates these through OCR and screenshots instead:

```bash
forepaw ocr-click "LIBRARY" --app Steam             # text navigation works via OCR
forepaw ocr-click "Shelter" --app Spotify --double  # double-click to play a song
forepaw click 310,420,80,70 --app Spotify           # region click for icon buttons
forepaw hover 325,410,60,60 --app Spotify           # region hover for tooltip discovery
```

**Region targeting** (`click x,y,w,h` / `hover x,y,w,h`) solves the icon button problem. LLMs can't predict precise pixel coordinates from screenshots, but they can draw rough bounding boxes. forepaw analyzes pixel saturation in the region, finds the centroid of the most colorful element (buttons are colored, backgrounds aren't), and clicks or hovers it. No vision model required. Hover region triggers hover states -- follow with a screenshot to see tooltip text rendered on screen.

Multi-process apps like Steam render their UI in a helper process. forepaw discovers these windows automatically -- `--app Steam` just works.

## Annotated screenshots

Three styles for bridging what's visible and what's interactive:

| Style | Use case |
|-------|----------|
| `--style badges` | Small numbered pills. Compact. Default with `--annotate`. |
| `--style labeled` | Bounding boxes with role and name. Human-readable. |
| `--style spotlight` | Dims non-interactive areas. Focus mode. |

```bash
forepaw screenshot --app Finder --annotate                          # badges on all elements
forepaw screenshot --app Finder --style spotlight --only @e1 @e3    # highlight specific refs
forepaw screenshot --app Finder --ref @e5 --padding 40              # crop to one element
```

Labels are color-coded: green for buttons, yellow for text fields, blue for selection controls, purple for navigation. Each label maps to an `@e` ref in a printed legend.

## Snapshot diffing

After an action, `--diff` shows what changed without re-reading the full tree:

```bash
forepaw snapshot --app Finder -i        # auto-cached
forepaw click @e3 --app Finder          # do something
forepaw snapshot --app Finder -i --diff # what changed?
```

Ref shifts are handled -- if a new element bumps all subsequent refs, unchanged elements still show as unchanged. `--context N` adds surrounding lines for spatial context.

## Batch actions

A raccoon doesn't open the lid, walk away, come back, reach in, walk away, come back, grab the food. Separate CLI invocations return control to the terminal between commands, which steals focus from the target app. Batch keeps focus throughout:

```bash
# Click a text field, type into it, press Enter -- all without losing focus
forepaw batch --app Notes "click @e3 ;; keyboard-type hello world ;; press return"

# Navigate to a URL
forepaw batch --app Orion "click 626,72 ;; keyboard-type example.com ;; press return"
```

Actions are separated by `;;`. Default 100ms delay between them (`--delay` to adjust). `--app` applies to all actions unless overridden per-action.

## Multi-window support

```bash
forepaw list-windows --app Zed
# w-1234  Zed  "my-project"
# w-1235  Zed  "other-project"

forepaw snapshot --app Zed --window "my-project"   # by title substring
forepaw screenshot --app Zed --window w-1234       # by window ID
```

Without `--window`, commands target the largest window. Ambiguous matches are reported with all candidates.

## Requirements

- Rust toolchain (for building)
- macOS 14+ (Apple Silicon or Intel)
- Two permissions on macOS, granted to your terminal app:

| Permission | Needed for | Where to grant |
|-----------|-----------|---------------|
| Accessibility | snapshot, click, type, hover | System Settings > Privacy & Security > Accessibility |
| Screen Recording | screenshot, ocr, ocr-click | System Settings > Privacy & Security > Screen & System Audio Recording |

Windows has no equivalent permission gates for UI Automation or input simulation.

```bash
forepaw permissions          # check status
forepaw permissions --request  # trigger system dialogs (macOS only)
```

## Design decisions

- **Accessibility-first.** Feel first, look second. A text tree is ~50 lines. A screenshot is ~1500 tokens. forepaw defaults to the cheaper, more precise option. OCR is the fallback, not the primary strategy.
- **CLI, not library or daemon.** Works with any language, any agent framework, any automation tool that can shell out. No SDK lock-in, no protocol to implement.
- **AX actions before mouse simulation.** `AXPress` doesn't move the physical cursor. More reliable, less disruptive. Mouse is the fallback.
- **Platform-agnostic core.** The ref system, tree rendering, diffing, and output formatting live in `src/core/` with no platform imports. `src/platform/darwin/` handles macOS (AXUIElement, CGEvent, Vision OCR), `src/platform/windows/` handles Windows (UIA, Win32). A Linux backend (AT-SPI2) would plug in with the same CLI.
- **Built for agents, useful for humans.** Raccoons are generalists. forepaw reads the same tree that VoiceOver does. Annotated screenshots make invisible structure visible -- useful for AI agents, but also for sighted people helping blind users debug UIs, developers auditing accessibility, or anyone trying to understand an unfamiliar app's interactive structure.

## Further reading

| Document | Contents |
|----------|----------|
| `docs/internals.md` | How it works under the hood -- AX batching, name resolution, pruning, coordinate systems. With raccoons. |
| `docs/performance-macos.md` | Benchmark data across apps, what's fast, what's slow, why Music is cursed. |
| `docs/cross-platform.md` | Linux and Windows feasibility research, AT-SPI2/UIA notes. |

## Development

Uses [mise](https://mise.jdx.dev) for task running and Cargo for building.

```bash
mise run check          # lint + test (run before committing)
mise run dev <command>  # build + run (e.g. mise run dev snapshot --app Finder -i)
mise run fmt            # auto-format (cargo fmt + swift-format for test apps)
```

Or without mise:

```bash
cargo build && cargo test
cargo clippy -- -D warnings
cargo fmt
```

### Cross-compiling for Windows

Build Windows binaries from any platform using [cargo-xwin](https://github.com/rust-cross/cargo-xwin) (downloads MSVC CRT + Windows SDK automatically):

```bash
# Install targets
rustup target add aarch64-pc-windows-msvc x86_64-pc-windows-msvc

# Build (requires cargo-xwin and lld)
cargo xwin build --target aarch64-pc-windows-msvc
cargo xwin build --target x86_64-pc-windows-msvc
```

Install `cargo-xwin` and `lld` however you prefer:

| Method | Command |
|--------|----------|
| Cargo | `cargo install cargo-xwin` |
| Nix | `nix shell nixpkgs#cargo-xwin nixpkgs#lld` |

The project includes a [nix flake](flake.nix) and [direnv](.envrc) config that provides `cargo-xwin` and `lld` automatically if you use them.

### Project layout

```
src/
  main.rs             # CLI entry point (clap derive)
  cli/                # Command handlers
  core/               # Platform-agnostic: types, refs, rendering, diffing
  platform/
    mod.rs            # DesktopProvider trait definition
    darwin/           # macOS: AXUIElement, CGEvent, Vision OCR, CoreGraphics
    windows/          # Windows: UIA, EnumWindows, SendInput (in progress)
TestApps/             # SwiftUI test apps for manual testing
```

### Contributing guidelines

- `src/core/` must stay free of platform imports. All macOS code goes in `src/platform/darwin/`.
- New public APIs need a `DesktopProvider` trait method using platform-agnostic types (`Point`, `Rect`).
- Every new type or function in `src/core/` needs unit tests.
- `cargo clippy -- -D warnings` and `cargo test` must pass before committing.
