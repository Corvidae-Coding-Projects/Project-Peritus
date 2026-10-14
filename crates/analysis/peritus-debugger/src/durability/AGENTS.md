# crates/analysis/peritus-debugger/src/durability — navigation

## Responsibility

C0 aggregate binding, atomic transition commits, outbox fencing, and replay.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Stable C0 identities and cross-record transition validation. |
| [commit.rs](commit.rs) | Atomic family-83 event, family-84 checkpoint, artifact, and outbox persistence. |
| [directive.rs](directive.rs) | Stable opaque C0 outbox directives and exact claim fences. |
| [mod.rs](mod.rs) | C0 aggregate binding, atomic transition commits, outbox fencing, and replay. |
| [replay.rs](replay.rs) | Checked C0 loading and exact complete-checkpoint recovery. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [commit/](commit/AGENTS.md) | Implementation pieces for `commit.rs`: Atomic family-83 event, family-84 checkpoint, artifact, and outbox persistence. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
