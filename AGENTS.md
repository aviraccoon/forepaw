# AGENTS.md

Desktop automation CLI for AI agents. Swift, macOS-first with cross-platform interface design.

## Quick Reference

```bash
swift build                              # Build
swift test                               # Run tests (ForepawCore only)
swift run forepaw <command>              # Run CLI
swift run forepaw snapshot --app Finder -i  # Quick smoke test
xcrun swift-format lint -r Sources/ Tests/  # Lint
xcrun swift-format format -i -r Sources/ Tests/  # Auto-format
```

## Key Paths

| Task | Location |
|------|----------|
| Add/modify CLI commands | `Sources/Forepaw/Commands+*.swift` |
| Platform-agnostic types & logic | `Sources/ForepawCore/` |
| macOS AX/OCR/input implementation | `Sources/ForepawDarwin/` |
| Core tests | `Tests/ForepawCoreTests/` |

## Project Context

- **Swift 6** with strict concurrency. `DesktopProvider` is `Sendable`.
- **swift-argument-parser** for CLI. Subcommand pattern matching `agent-browser`.
- **No external dependencies** beyond ArgumentParser. macOS APIs (AXUIElement, CGEvent, screencapture, Vision) used directly.
- **Ref system**: `@e1`, `@e2` assigned depth-first by `RefAssigner`. Positional -- action commands re-walk the tree to resolve refs across CLI invocations.
- **AX-first actions**: `click` tries `AXPress` before CGEvent mouse fallback. Exception: web content links use mouse-first (AXPress doesn't trigger browser navigation).
- **OCR via Vision framework**: `VNRecognizeTextRequest` on window screenshots. Coordinates need Retina scale factor (backingScaleFactor) and window origin offset for screen-space clicks.
- **Two permissions**: Accessibility (for AX tree, actions) and Screen Recording (for screenshots, OCR). Both checked in `forepaw permissions`.

## Releases

- Each release gets a codename from the extended raccoon family -- raccoons, possums, coatis, kinkajous, olingos, ringtails, tanuki, civets, binturongs, red pandas, etc.
- Format in CHANGELOG.md: `## v0.2.0 "Ringtail" (2026-04-15)`
- Keep it playful.

## Formatting

- **swift-format** (ships with Xcode toolchain). Config in `.swift-format`.
- 4-space indent, 120 char line length.
- Run `xcrun swift-format format -i -r Sources/ Tests/` before committing.
- Lint with `xcrun swift-format lint -r Sources/ Tests/` -- must be zero warnings.

## Guidelines

- Keep `ForepawCore` free of platform imports (`ApplicationServices`, `Cocoa`, `Carbon`, `Vision`). All macOS-specific code goes in `ForepawDarwin`. The CLI target (`Forepaw`) also stays platform-agnostic -- no `Cocoa` imports.
- Mirror `agent-browser`'s CLI patterns where applicable (same flag names, similar output format, `@e` ref syntax).
- `--app` activates the target app before mouse/keyboard actions. Make it optional for commands where global input makes sense (e.g. `press` for system hotkeys, `keyboard-type` for typing into current focus).
- Test `ForepawCore` logic (ref assignment, tree rendering, key parsing) with unit tests. `ForepawDarwin` tests need interactive accessibility access -- keep them separate.
- Output is plain text by default, `--json` for structured JSON.
- Element names: check `AXTitle`, then `AXDescription`, then `AXTitleUIElement` (points to a label element), then first `AXStaticText` child's `AXValue`. This chain (`computedName`) handles cells, rows, and other container elements.
- Keystroke simulation needs inter-character delay (~8ms) for Electron apps. Without it, characters get dropped.
- **Every feature or behavior change must update the agent skill** (`.agents/skills/forepaw/SKILL.md`) and `README.md`. The skill is how agents learn to use forepaw -- if a capability isn't documented there, it doesn't exist to them.
