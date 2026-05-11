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
   Codenames come from the extended raccoon family: raccoons, possums, coatis, kinkajous, olingos, ringtails, tanuki, civets, binturongs, red pandas, etc. Keep it playful.

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

7. **CI handles the rest** -- `.github/workflows/release.yml` builds a release binary on `macos-26`, extracts release notes from CHANGELOG.md, and uploads to GitHub Releases with install instructions.

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
