# crates/foundation/peritus-budget/src/transition/reconciliation — navigation

## Responsibility

Implementation pieces for `reconciliation.rs`: Reservation activation, observation, and terminal reconciliation.

## Files in this folder

| File | Role |
| --- | --- |
| [finalization.rs](finalization.rs) | Exact and conservative reservation finalization transitions. |
| [observation.rs](observation.rs) | Monotonic high-water observation transitions. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [finalization/](finalization/AGENTS.md) | Implementation pieces for `finalization.rs`: Exact and conservative reservation finalization transitions. |
| [observation/](observation/AGENTS.md) | Implementation pieces for `observation.rs`: Monotonic high-water observation transitions. |

Owning module: [reconciliation.rs](../reconciliation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
