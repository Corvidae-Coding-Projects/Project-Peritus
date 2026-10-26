# crates/orchestration/peritus-harness/src/durability — navigation

## Responsibility

C0 binding, atomic commits, and checked recovery loading.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Cross-record harness transition binding validation. |
| [commit.rs](commit.rs) | Atomic family-80 event, family-81 checkpoint, artifact, and outbox persistence. |
| [error.rs](error.rs) | Typed C0 commit and recovery failures. |
| [mod.rs](mod.rs) | C0 binding, atomic commits, and checked recovery loading. |
| [recovery.rs](recovery.rs) | Checked C0 loading and exact complete-checkpoint recovery. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
