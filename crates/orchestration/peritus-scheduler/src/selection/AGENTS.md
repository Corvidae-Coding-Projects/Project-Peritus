# crates/orchestration/peritus-scheduler/src/selection — navigation

## Responsibility

Implementation pieces for `selection.rs`: Pure deterministic bounded-bypass selection.

## Files in this folder

| File | Role |
| --- | --- |
| [capacity.rs](capacity.rs) | Exact checked reservation sums and prospective capacity predicates. |
| [model.rs](model.rs) | Exact mathematical model for production scheduler selection. |
| [scan.rs](scan.rs) | Exact feasibility scans used by deterministic selection. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [capacity/](capacity/AGENTS.md) | Implementation pieces for `capacity.rs`: Exact checked reservation sums and prospective capacity predicates. |
| [scan/](scan/AGENTS.md) | Implementation pieces for `scan.rs`: Exact feasibility scans used by deterministic selection. |

Owning module: [selection.rs](../selection.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
