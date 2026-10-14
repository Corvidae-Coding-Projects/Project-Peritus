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

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
