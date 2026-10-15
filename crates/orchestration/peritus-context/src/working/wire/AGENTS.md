# crates/orchestration/peritus-context/src/working/wire — navigation

## Responsibility

Versioned C0-canonical runtime encoding; deterministic semantics remain in the reducers.

## Files in this folder

| File | Role |
| --- | --- |
| [entry.rs](entry.rs) | Canonical source-backed entry fields; model deltas cannot set host-derived status. |
| [event.rs](event.rs) | Standalone canonical working-event frames with exact scope checks. |
| [fields.rs](fields.rs) | Canonical checked identity, source, and environment fields. |
| [mod.rs](mod.rs) | Versioned C0-canonical runtime encoding; deterministic semantics remain in the reducers. |
| [protocol.rs](protocol.rs) | Canonical literal pins and pending-operation dispositions. |
| [state.rs](state.rs) | Exact working snapshots, with bounded reconstruction and graph/status validation. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
