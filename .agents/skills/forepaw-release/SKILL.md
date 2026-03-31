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

2. **Update version in `Sources/Forepaw/Version.swift`** -- change `baseVersion`:
   ```swift
   let baseVersion = "0.X.0"
   ```

3. **Update docs** -- make sure README.md, `.agents/skills/forepaw/SKILL.md`, and AGENTS.md reflect all changes since last release.

4. **Build and test**:
   ```bash
   swift build
   swift test
   xcrun swift-format lint -r Sources/ Tests/
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
- Dev builds auto-append git hash: `0.2.0-dev+abc1234`
- Release builds use the tag: `v0.2.0`

## Previous codenames

Check existing codenames to avoid reuse:
```bash
rg '^## v' CHANGELOG.md
```
