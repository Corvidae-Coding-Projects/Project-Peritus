# crates/analysis/peritus-eval/src/durability — navigation

## Responsibility

C0 aggregate binding, outbox directives, atomic commits, and replay.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Stable C0 identities and cross-record transition validation. |
| [commit.rs](commit.rs) | Atomic family-86 event, family-87 checkpoint, artifact, and outbox persistence. |
| [directive.rs](directive.rs) | Stable C0 schedule, execution, cancellation, and publication directives. |
| [mod.rs](mod.rs) | C0 aggregate binding, outbox directives, atomic commits, and replay. |
| [replay.rs](replay.rs) | Checked C0 loading and exact complete-checkpoint replay. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
