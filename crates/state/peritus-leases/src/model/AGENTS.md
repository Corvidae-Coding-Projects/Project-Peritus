# crates/state/peritus-leases/src/model — navigation

## Responsibility

Implementation pieces for `model.rs`: Abstract reachability model and mathematical lease predicates.

## Files in this folder

| File | Role |
| --- | --- |
| [concrete.rs](concrete.rs) | Refinement predicates over the executable lease representation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [concrete/](concrete/AGENTS.md) | Implementation pieces for `concrete.rs`: Refinement predicates over the executable lease representation. |

Owning module: [model.rs](../model.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
