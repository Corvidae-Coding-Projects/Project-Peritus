# crates/orchestration/peritus-scheduler/src/verified/reservations/clone — navigation

## Responsibility

Implementation pieces for `clone.rs`: Reservation-invariant transport across the actual semantic state clone.

## Files in this folder

| File | Role |
| --- | --- |
| [arithmetic.rs](arithmetic.rs) | Exact capacity transport across pointwise-equivalent reservation sequences. |

Owning module: [clone.rs](../clone.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
