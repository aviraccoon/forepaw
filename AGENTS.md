# AGENTS.md

Desktop automation CLI for AI agents. Swift, macOS-first with cross-platform interface design.

## Quick Reference

```bash
swift build                              # Build
swift test                               # Run tests (ForepawCore only)
swift run forepaw <command>              # Run CLI
swift run forepaw snapshot --app Finder -i  # Quick smoke test
xcrun swift-format lint -r Sources/ Tests/ TestApps/  # Lint
xcrun swift-format format -i -r Sources/ Tests/ TestApps/  # Auto-format
mise run check                           # Lint + build + test
mise run dev <command>                   # Build + run (e.g. mise run dev snapshot --app Finder -i)
```

## Key Paths

| Task | Location |
|------|----------|
| Add/modify CLI commands | `Sources/Forepaw/Commands+*.swift` |
| Platform-agnostic types & logic | `Sources/ForepawCore/` |
| macOS AX/OCR/input implementation | `Sources/ForepawDarwin/` |
| Core tests | `Tests/ForepawCoreTests/` |
| Test apps (SwiftUI, manual testing) | `TestApps/` |

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
- Run `xcrun swift-format format -i -r Sources/ Tests/ TestApps/` before committing.
- Lint with `xcrun swift-format lint -r Sources/ Tests/ TestApps/` -- must be zero warnings.

## Guidelines

- Keep `ForepawCore` free of platform imports (`ApplicationServices`, `Cocoa`, `Carbon`, `Vision`). All macOS-specific code goes in `ForepawDarwin`. The CLI target (`Forepaw`) also stays platform-agnostic -- no `Cocoa` imports.
- **Every new public API in `ForepawDarwin` must have a corresponding method on the `DesktopProvider` protocol in `ForepawCore`.** Use platform-agnostic types (`Point`, `Rect`, not `CGPoint`, `CGRect`) in the protocol. Convert to platform types inside the Darwin implementation. The CLI target should only depend on `ForepawCore` types.
- Mirror `agent-browser`'s CLI patterns where applicable (same flag names, similar output format, `@e` ref syntax).
- `--app` activates the target app before mouse/keyboard actions. Make it optional for commands where global input makes sense (e.g. `press` for system hotkeys, `keyboard-type` for typing into current focus).
- Test `ForepawCore` logic (ref assignment, tree rendering, key parsing) with unit tests. `ForepawDarwin` tests need interactive accessibility access -- keep them separate.
- **Every new type or function in `ForepawCore` needs unit tests.** Adding `DragOptions`? Add `DragOptionsTests.swift`. Adding `parseModifiers`? Test it. The test suite is the safety net for the cross-platform core -- if it's not tested, it'll break silently when refactored.
- Output is plain text by default, `--json` for structured JSON.
- Element names: check `AXTitle`, then `AXDescription`, then `AXTitleUIElement` (points to a label element), then first `AXStaticText` child's `AXValue`. This chain (`computedName`) handles cells, rows, and other container elements.
- Keystroke simulation needs inter-character delay (~8ms) for Electron apps. Without it, characters get dropped.
- **User text arguments that could start with dashes** need `@Option(parsing: .unconditional)` as an alternative to the positional `@Argument`. ArgumentParser treats dash-prefixed values as flags, so a positional `@Argument` can't accept text like `"--verbose"`. The pattern: keep the positional as optional for normal text, add `@Option(name: .customLong("text"), parsing: .unconditional)` as a named alternative, and use `resolveText()` to pick one. See `keyboard-type`, `type`, `ocr-click`, `wait` for examples.
- **Every feature or behavior change must update the agent skill** (`.agents/skills/forepaw/SKILL.md`) and `README.md`. The skill is how agents learn to use forepaw -- if a capability isn't documented there, it doesn't exist to them.
- **Load the forepaw skill before testing interactively.** The skill documents the observe-act loop, command patterns, and behavioral gotchas. Read it before running forepaw commands against real apps.
