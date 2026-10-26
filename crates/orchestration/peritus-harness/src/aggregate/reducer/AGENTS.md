# crates/orchestration/peritus-harness/src/aggregate/reducer — navigation

## Responsibility

Implementation pieces for `reducer.rs`: Pure checked command decision and event replay reducer.

## Files in this folder

| File | Role |
| --- | --- |
| [settlement.rs](settlement.rs) | Plan validation and terminal materialization settlement transitions. |

Owning module: [reducer.rs](../reducer.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
