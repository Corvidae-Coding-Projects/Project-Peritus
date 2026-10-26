# crates/orchestration/peritus-scheduler/src/state/mutation/entity_insertion — navigation

## Responsibility

Implementation pieces for `entity_insertion.rs`: Production worker and work insertion with actual-state preservation contracts.

## Files in this folder

| File | Role |
| --- | --- |
| [slots.rs](slots.rs) | Exact canonical insertion positions from the production binary-search algorithm. |

Owning module: [entity_insertion.rs](../entity_insertion.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
