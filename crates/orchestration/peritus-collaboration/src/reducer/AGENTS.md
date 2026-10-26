# crates/orchestration/peritus-collaboration/src/reducer — navigation

## Responsibility

Implementation pieces for `reducer.rs`: Pure deterministic collaboration transition and exact replay.

## Files in this folder

| File | Role |
| --- | --- |
| [apply.rs](apply.rs) | Closed collaboration command application separated from fence and replay orchestration. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [apply/](apply/AGENTS.md) | Implementation pieces for `apply.rs`: Closed collaboration command application separated from fence and replay orchestration. |

Owning module: [reducer.rs](../reducer.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
