# crates/model/peritus-provider-anthropic/src/bin/claude_runtime_fake — navigation

## Responsibility

Implementation pieces for `claude_runtime_fake.rs`: Portable hermetic executable used only by this package's runtime conformance tests.

## Files in this folder

| File | Role |
| --- | --- |
| [mod.rs](mod.rs) | Test-only Claude command behavior and invocation assertions. |

Owning module: [claude_runtime_fake.rs](../claude_runtime_fake.rs). Inspect that parent before changing the local pieces.

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
