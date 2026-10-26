# crates/orchestration/peritus-scheduler/src/verified/reservations — navigation

## Responsibility

Implementation pieces for `reservations.rs`: Actual-state reservation capacity and ownership model for OBL-0149.

## Files in this folder

| File | Role |
| --- | --- |
| [arithmetic.rs](arithmetic.rs) | Sequence arithmetic used by reservation insertion and release proofs. |
| [clone.rs](clone.rs) | Reservation-invariant transport across the actual semantic state clone. |
| [entity_insertion.rs](entity_insertion.rs) | Preservation when production admission retains a new worker or work item. |
| [feasibility.rs](feasibility.rs) | Exact admission relation for one new durable reservation. |
| [insertion.rs](insertion.rs) | Preservation proof for adding one selected feasible reservation. |
| [preservation.rs](preservation.rs) | Public preservation adapters for the OBL-0149 reservation model. |
| [readiness.rs](readiness.rs) | Reducer admission facts connecting lifecycle and durable dispatch history. |
| [removal.rs](removal.rs) | Preservation proof for releasing one exact reservation. |
| [work_update.rs](work_update.rs) | Preservation proof for reservation-stable work lifecycle updates. |
| [worker_update.rs](worker_update.rs) | Preservation proof for reservation-stable worker lifecycle updates. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [clone/](clone/AGENTS.md) | Implementation pieces for `clone.rs`: Reservation-invariant transport across the actual semantic state clone. |
| [readiness/](readiness/AGENTS.md) | Implementation pieces for `readiness.rs`: Reducer admission facts connecting lifecycle and durable dispatch history. |

Owning module: [reservations.rs](../reservations.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
