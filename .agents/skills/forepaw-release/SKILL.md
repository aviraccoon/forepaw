---
name: forepaw-release
description: Release a new version of forepaw. Use when asked to tag, release, or publish a new version.
---

# Releasing forepaw

## Steps

1. **Update CHANGELOG.md** -- add a section for the new version at the top:
   ```
   ## v0.X.0 "Codename" (YYYY-MM-DD)
   ```
   Codenames should be playful and reflect the release's character. They don't have to be real raccoon-family species — invent a hybrid, borrow from memes, make a pun, or come up with something that captures the release's spirit. Examples (real and invented):

- Pure animal species: `Coatis`, `Kinkajous`, `Binturong`
- Nicknames: `Trash Panda`, `Masked Bandit`
- Invented hybrids: `The Trash Crab` (Trash Panda + Rust crab for a Rust rewrite)
- Playful phrases: `Ferris Goes Foraging`, `Everything is Crab`

The only rule: it should be playful and fit the release's vibe. Avoid being boring or purely technical (no `v0.4.0 "Cross-Platform"`).

2. **Update version in `Cargo.toml`** -- change the `version` field:
   ```toml
   version = "0.X.0"
   ```
   The CLI reads this via `env!("CARGO_PKG_VERSION")` -- no other file needs a version bump.

3. **Update docs** -- make sure README.md, `.agents/skills/forepaw/SKILL.md`, and AGENTS.md reflect all changes since last release.

4. **Build and test**:
   ```bash
   cargo build
   cargo test
   cargo clippy -- -D warnings
   cargo fmt --check
   ```

5. **Commit the release**:
   ```bash
   git add -A
   git commit -m "release: v0.X.0 \"Codename\""
   ```

6. **Tag and push**:
   ```bash
   git tag v0.X.0
   git push origin main --tags
   ```

7. **CI handles the rest** -- `.github/workflows/release.yml`:
   - Builds release binaries on macOS, Windows, and Linux
   - Extracts release notes from CHANGELOG.md
   - Uploads to GitHub Releases with install instructions
   - Publishes `forepaw` library crate to crates.io via Trusted Publishing (OIDC)

   The CLI binary crate (`forepaw-cli`) has `publish = false` and is distributed via GitHub Releases, not crates.io.

8. **Update Nix package** (in the system repo) -- update `packages/forepaw.nix` with the new version and tarball SHA256 from the release.

## Version scheme

- Semver: `MAJOR.MINOR.PATCH`
- Version is defined once in `Cargo.toml`, read by the CLI via `env!("CARGO_PKG_VERSION")`
- Dev builds are whatever `Cargo.toml` says (no git hash suffix)

## Previous codenames

Check existing codenames to avoid reuse:
```bash
rg '^## v' CHANGELOG.md
```
