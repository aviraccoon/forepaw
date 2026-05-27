# AGENTS.md

Desktop automation CLI for AI agents. Rust, macOS-first with cross-platform interface design.

## Quick Reference

```bash
mise run check              # Lint + test (use before committing)
mise run dev <command>      # Build + run (e.g. mise run dev snapshot --app Finder -i)
mise run fmt                # Auto-format (Rust + Swift test apps)
mise run lint               # Lint current platform (clippy)
mise run lint-all           # Lint all platform targets (needs rustup targets)
mise run build              # Build only
mise run test               # Test only
```

No external task runner required -- Cargo is the build system. Mise tasks wrap Cargo for convenience.

## Key Paths

| Task | Location |
|------|----------|
| Add/modify CLI commands | `src/cli/action.rs`, `src/cli/observation.rs`, `src/cli/system.rs` |
| Platform-agnostic types & logic | `src/core/` |
| Logging (FOREPAW_LOG env var) | `src/log.rs` |
| Platform abstraction (DesktopProvider trait) | `src/platform/mod.rs` |
| macOS backend (AX, OCR, input, screenshots) | `src/platform/darwin/` |
| Windows backend (UIA, Win32, WinRT OCR) | `src/platform/windows/` |
| Linux backend (AT-SPI2, D-Bus) | `src/platform/linux/` |
| AT-SPI2 role generator | `res/generate_atspi_roles.sh` → `src/platform/linux/role.rs` |
| Test apps (SwiftUI, manual testing) | `TestApps/` |
| Research docs | `docs/` |
| Windows diagnostic scripts | `scripts/windows/` |
| Nix flake (build, dev shell, formatter) | `flake.nix` |

## Project Context

- **Rust edition 2021**, strict clippy.
- **clap** derive for CLI. Subcommand pattern matching `agent-browser`.
- **Dependencies**: clap, anyhow, serde, serde_json. Platform APIs via `objc2` (macOS), `windows` crate (Windows), `zbus` (Linux). No external cross-platform deps.
- **Ref system**: `@e1`, `@e2` assigned depth-first by `RefAssigner`. Positional -- action commands re-walk the tree to resolve refs across CLI invocations.
- **DesktopProvider trait** in `src/platform/mod.rs` defines the full platform surface. All CLI commands call through `&dyn DesktopProvider`. Every new platform method must be added to the trait first.
- **Single crate with cfg gates** for all three platforms (`target_os = "macos"` / `"windows"` / `"linux"`).
- **`r#ref` everywhere** because `ref` is a Rust keyword.
- **Two permissions**: Accessibility (for AX tree, actions) and Screen Recording (for screenshots, OCR). Both checked in `forepaw permissions`.

## Releases

- Each release gets a codename from the extended raccoon family -- raccoons, possums, coatis, kinkajous, olingos, ringtails, tanuki, civets, binturongs, red pandas, etc.
- Format in CHANGELOG.md: `## v0.2.0 "Ringtail" (2026-04-15)`
- Keep it playful.

## Formatting

- **rustfmt** (ships with Rust toolchain). Default settings.
- **nixfmt** for `.nix` files (via `nix fmt -- flake.nix` or the dev shell).
- **Run `mise run check` before every commit.** No exceptions. It checks fmt, clippy, and tests in order.
- Zero clippy warnings: `cargo clippy` must pass clean on all platform targets you changed. Use `mise run lint-all` to check all targets.
- Swift test apps use `swift-format` (via `mise run fmt`).

## Guidelines

- Keep `src/core/` free of platform imports. All platform-specific code goes in `src/platform/`.
- **Every new public API in `src/platform/` must have a corresponding method on the `DesktopProvider` trait.** Use platform-agnostic types (`Point`, `Rect`, not `CGPoint`, `CGRect`) in the trait. Convert to platform types inside the Darwin implementation. The CLI should only depend on trait types.
- **Read skill files completely before using the tool.** The skill description says "read this before running any forepaw command" -- that means the full file, not skimming. Skills contain behavioral rules, gotchas, and patterns that prevent errors. A partial read leads to misused flags, wrong coordinate systems, and broken workflows.
- Mirror `agent-browser`'s CLI patterns where applicable (same flag names, similar output format, `@e` ref syntax).
- `--app` activates the target app before mouse/keyboard actions. Make it optional for commands where global input makes sense (e.g. `press` for system hotkeys, `keyboard-type` for typing into current focus).
- Every new type or function in `src/core/` needs unit tests. Test pure logic even when FFI-dependent code needs a live app.
- Output is plain text by default, `forepaw -f json <command>` for structured JSON (global `--format` flag).
- Element names: check `AXTitle`, then `AXDescription`, then `AXTitleUIElement` (points to a label element), then first `AXStaticText` child's `AXValue`. This chain (`computedName`) handles cells, rows, and other container elements.
- Keystroke simulation needs inter-character delay (~8ms) for Electron apps. Without it, characters get dropped.
- **Every feature or behavior change must update the agent skill** (`.agents/skills/forepaw/SKILL.md`) and `README.md`. The skill is how agents learn to use forepaw -- if a capability isn't documented there, it doesn't exist to them.
- **Load the forepaw skill before testing interactively.** The skill documents the observe-act loop, command patterns, and behavioral gotchas. Read it before running forepaw commands against real apps.
- **Coordinate-based actions validate against window bounds.** `click_at_point` and `hover_at_point` reject coordinates outside the target window (errors, not clamps -- a misplaced click could be destructive). Any new coordinate-based action must validate when `--app` is specified.
- Implement `std::str::FromStr` for string-parsed enums (clippy enforces this over custom `from_str` methods).
- Use `anyhow::Result` in CLI command methods; use `Result<_, ForepawError>` in platform/trait methods.
- **Per-site `#[expect]` for cast lints, never fn-wide.** Fn-wide `#[expect(clippy::cast_*)]` silently suppresses new casts added later. Always annotate the specific `as` expression with `#[expect(clippy::cast_X, reason = "why this is safe")]`. For display-only casts (format strings), prefer eliminating the cast entirely by formatting f64 directly with `{:.0}` instead of casting to `i32`/`i64` first.
- `forepaw-audit` and other companion tools depend on this crate as a library dependency (not subprocess/JSON). Keep the lib surface clean.
- **Debug logging**: `FOREPAW_LOG=debug` or `FOREPAW_LOG=snapshot=debug`. Zero-deps, uses `RUST_LOG` as fallback. See `src/log.rs`.
- **Read docs and skill files in full before acting on them.** Skimming leads to stale assumptions and wrong edits.
- **Nix**: `nix build` produces the binary, `nix develop` gives a complete dev environment (Rust + cross-compilation tools), `nix fmt` formats `.nix` files. Linux CI uses the nix dev shell for musl builds. Tests that depend on PATH tools (e.g. `is_command_available`) should skip in the Nix sandbox (`NIX_BUILD_TOP` env check).
