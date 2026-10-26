# crates/analysis/peritus-debugger/src/aggregate/reducer — navigation

## Responsibility

Implementation pieces for `reducer.rs`: Pure command decision and deterministic event replay.

## Files in this folder

| File | Role |
| --- | --- |
| [apply.rs](apply.rs) | Exhaustive command-to-event/state reduction. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [apply/](apply/AGENTS.md) | Implementation pieces for `apply.rs`: Exhaustive command-to-event/state reduction. |

Owning module: [reducer.rs](../reducer.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
