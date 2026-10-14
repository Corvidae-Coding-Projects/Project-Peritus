# crates/orchestration/peritus-scheduler/src/verified — navigation

## Responsibility

Implementation pieces for `verified.rs`: Executable invariant/refinement predicates for the local domain. Entry names: `dependencies_are_ready`, `reservations_fit`, `resource_sum`, `unique_dispatch_ownership`, `attempts_are_monotonic`.

## Files in this folder

| File | Role |
| --- | --- |
| [reservation_transitions.rs](reservation_transitions.rs) | Actual-state adapters for the OBL-0149 preservation lemmas. |
| [reservations.rs](reservations.rs) | Actual-state reservation capacity and ownership model for OBL-0149. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [reservations/](reservations/AGENTS.md) | Implementation pieces for `reservations.rs`: Actual-state reservation capacity and ownership model for OBL-0149. |

Owning module: [verified.rs](../verified.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
