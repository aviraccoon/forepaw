# Performance: macOS Accessibility

How fast (or slow) forepaw's accessibility tree queries are on macOS, what affects performance, and what we've done about it.

The optimizations here are specific to macOS's `AXUIElement` API and its synchronous Mach IPC model. Linux (AT-SPI2 over D-Bus) and Windows (UIA over COM) have different IPC patterns and will need their own performance analysis. See `cross-platform.md` for platform details.

## The bottleneck

Snapshot time is almost entirely IPC wait. Each `AXUIElementCopyMultipleAttributeValues` call sends a Mach message to the target app's process and waits for a response. forepaw's own CPU time is negligible (~0.2s user time even for 1600-node trees).

Performance depends on the target app's accessibility responder, not on forepaw.

## App benchmarks

Measured with 13-attribute batch fetch per element. April 2026.

**Test machine:** MacBook Pro M4 Pro, 48GB RAM, macOS Tahoe 26.4.

| App | Framework | Nodes | Time | ms/node |
|-----|-----------|-------|------|---------|
| Telegram | Native (AppKit) | 208 | 0.015s | 0.1 |
| Bruno | Electron | 334 | 0.019s | 0.1 |
| 1Password | Electron | 299 | 0.038s | 0.1 |
| Mona 6 | Native (AppKit) | 399 | 0.041s | 0.1 |
| Discord | Electron | 933 | 0.044s | <0.1 |
| Finder | Native (AppKit) | 641 | 0.047s | 0.1 |
| Obsidian | Electron | 962 | 0.046s | <0.1 |
| Maps | Catalyst | 346 | 0.052s | 0.2 |
| Messages | Catalyst | 360 | 0.070s | 0.2 |
| Podcasts | Catalyst | 320 | 0.047s | 0.1 |
| Stocks | Catalyst | 339 | 0.051s | 0.2 |
| Books | Catalyst | 308 | 0.032s | 0.1 |
| MacWhisper | Native | 543 | 0.404s | 0.7 |
| System Settings | SwiftUI | 428 | 0.435s | 1.0 |
| Orion | WebKit | 1921 | 0.618s | 0.3 |
| Music | Catalyst | 794 | 0.13s | 0.2 |
| Music (`--offscreen`) | Catalyst | 1608 | ~12-50s | 7-31 |

Most apps respond in under 100ms. System Settings (SwiftUI) and MacWhisper are slower per-node but still sub-second.

Music looks normal in the table because offscreen pruning (the default) skips its 800+ invisible play history rows. With `--offscreen`, Music is 100-300x slower per node than every other app -- see below.

### Music

Music exposes 200+ play history rows in its AX tree at negative Y coordinates (y=-9631) even when the queue shows "There's no music in the queue." Each invisible row triggers media library queries through the Catalyst AX bridge, making the full tree take 12-50s.

This is specific to Music, not to Catalyst. Other Catalyst apps (Maps, Messages, Podcasts, Stocks, Books) are all 0.1-0.2ms/node. The slowness comes from Music's media library backing store being queried per AX element.

With offscreen pruning (the default), Music snapshots complete in under a second.

### Diagnosing slow snapshots

Use `--timing` to see where time is spent:

```
$ forepaw snapshot --app Music --timing --offscreen >/dev/null
snapshot: 28738ms, 1608 nodes, 17.9ms/node avg
  AXWindow "Music"  1278 nodes   79.5%
    AXSplitGroup "split group"  1264 nodes   78.6%
      AXGroup "play queue"  1023 nodes   63.6%
        AXTable  1013 nodes   63.0%
  AXMenuBar   329 nodes   20.5%
```

The output adaptively expands large subtrees and collapses single-child chains (common in Electron apps). Printed to stderr so it doesn't interfere with the snapshot output.

## What makes it fast

### Offscreen pruning

Elements whose bounds are entirely outside the window rect are skipped. This is the biggest single optimization for apps that expose invisible content.

Apple Music exposes 200+ play history rows in its AX tree at negative Y coordinates (y=-9631) even when the queue shows "There's no music in the queue." Each invisible row triggers media library queries through the Catalyst AX bridge. Offscreen pruning skips these entirely.

| App | Mode | Nodes | Tree walk |
|-----|------|-------|-----------|
| Music | `--offscreen` (include all) | 1608 | ~30s |
| Music | default (skip offscreen) | 794 | 132ms |
| Music | `-i` (skip offscreen + menu + hidden) | 466 | 95ms |
| Orion | `--offscreen` | 1953 | 1.1s |
| Orion | default | 1638 | 0.8s |

Default-on in all modes. Use `--offscreen` to include them for debugging.

### Batched attribute fetching

`AXUIElementCopyMultipleAttributeValues` fetches 13 attributes in a single IPC round-trip instead of 13 individual `AXUIElementCopyAttributeValue` calls.

Before batching was expanded from 8 to 13 attributes, unnamed elements triggered 5 additional individual IPC calls for name resolution (`computedName`). On Music, this meant 6 IPC calls per element instead of 1 -- accounting for 77% of total time.

| App | Before (8-batch + individual) | After (13-batch) | Speedup |
|-----|-------------------------------|-------------------|---------|
| Music | ~50s | ~12s | ~4x |
| Orion | 1.5s | 0.6s | 2.4x |
| Electron apps | varies | varies | 1.5-1.9x |
| Native apps | varies | varies | 1.1-1.4x |

### Children-first name resolution

`buildTree` recurses into children before computing the parent's name. This lets `computedName` read from already-built `ElementNode` objects (step 2: first AXStaticText child's value, AXImage child's name) instead of making individual IPC calls to query each child's attributes. Those children were going to be built anyway -- this just reorders the work.

### Smart defaults (`-i` mode)

Interactive mode (`-i`) auto-skips:
- **Menu bar**: 200-300 elements that agents rarely need. `--menu` overrides.
- **Zero-size elements**: Collapsed menus, hidden panels. `--zero-size` overrides.

### Single-pass tree walk

The tree is walked once, building `ElementNode` objects and collecting `AXUIElement` handles for ref resolution simultaneously. Previously these were two separate walks.

## What doesn't help

### Parallel tree walking

Tested `DispatchQueue.concurrentPerform` on sibling subtrees at shallow depths. Results:

| App | Sequential | Parallel | Speedup |
|-----|------------|----------|---------|
| Music | 12s | 12s | 1.0x |
| Finder | 0.05s | 0.04s | 1.2x |
| Discord | 0.05s | 0.04s | 1.3x |

Music's AX responder serializes incoming Mach messages -- concurrent queries just queue up. Native apps get a small benefit but are already fast enough that it doesn't matter. The complexity isn't worth it.

### Observations

Apps with slow AX responders tend to be slow because of what's behind the AX bridge (database queries, media library access), not because of the bridge itself. No amount of query optimization on the client side can fix a slow server.

The framework hierarchy for macOS AX performance: **AppKit ≈ Electron ≈ Catalyst > SwiftUI >> Music**. SwiftUI's accessibility bridge adds ~10x overhead vs AppKit (1.0ms vs 0.1ms per node), but that's still well under a second for typical UIs.

Linux (AT-SPI2 over DBus) and Windows (UIA) will have different performance profiles. AT-SPI2 uses async DBus messages rather than synchronous Mach IPC, so the batching strategy may not apply the same way. Performance data for other platforms will be added as backends are implemented.
