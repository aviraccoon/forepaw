# AGENTS.md

Rust rewrite of forepaw (desktop automation CLI for AI agents). Orphan branch `rust/cancrivorus` in the forepaw repo worktree.

## Quick Reference

```bash
cargo build              # Build
cargo test               # Run tests (135 total)
cargo clippy             # Lint
cargo run -- --help      # CLI help
```

No external task runner. Cargo is the build system.

## Key Paths

| Task | Location |
|------|----------|
| Add/modify CLI commands | `src/cli/Commands+*.swift` → `src/cli/action.rs`, `src/cli/observation.rs`, `src/cli/system.rs` |
| Platform-agnostic types & logic | `src/core/` |
| Platform abstraction (DesktopProvider trait) | `src/platform/mod.rs` |
| Platform backends (future) | `src/platform/macos.rs`, `src/platform/windows.rs`, `src/platform/linux.rs` |

## Project Context

- **Rust edition 2021**, strict clippy.
- **clap** derive for CLI. Subcommand pattern matching the Swift CLI contract exactly.
- **No external deps** beyond clap, anyhow, regex. Platform APIs via `objc2`, `windows-rs`, `atspi` when backends land.
- **Ref system**: `@e1`, `@e2` assigned depth-first by `RefAssigner`. Positional -- action commands re-walk the tree to resolve refs across CLI invocations.
- **DesktopProvider trait** in `src/platform/mod.rs` defines the full platform surface. All CLI commands call through `&dyn DesktopProvider`. Every new platform method must be added to the trait first.
- **Single crate with cfg gates** for phase 1 (macOS parity). Workspace split happens when adding Windows/Linux backends.
- **`r#ref` everywhere** because `ref` is a Rust keyword.

## Guidelines

- Run `cargo clippy` and `cargo test` before committing.
- Keep `src/core/` free of platform imports. All platform-specific code goes in `src/platform/`.
- Every new public API in `src/platform/` must have a corresponding method on the `DesktopProvider` trait.
- Every new type, function, or constant in any module needs unit tests -- including platform backends (`src/platform/darwin/`). Test pure logic (helpers, pruning math, constants, name computation chains) even when the FFI-dependent tree walk itself needs a live app.
- Implement `std::str::FromStr` for string-parsed enums (clippy enforces this over custom `from_str` methods).
- Use `anyhow::Result` in CLI command methods; use `Result<_, ForepawError>` in platform/trait methods.
- Every new type or function in `src/core/` needs unit tests.
- Port features 1:1 from Swift unless there's a concrete reason to diverge.
- `forepaw-audit` and other companion tools depend on this crate as a library dependency (not subprocess/JSON). Keep the lib surface clean.


