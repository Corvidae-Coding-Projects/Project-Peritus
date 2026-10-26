# crates/orchestration/peritus-orchestrator/src/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Complete authoritative replayable E0 aggregate state.

## Files in this folder

| File | Role |
| --- | --- |
| [counters.rs](counters.rs) | Independently bounded E0 accounting counters. |
| [mutation.rs](mutation.rs) | Reducer-confined E0 state mutation primitives. |
| [validation.rs](validation.rs) | Complete decoded-checkpoint semantic validation. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
