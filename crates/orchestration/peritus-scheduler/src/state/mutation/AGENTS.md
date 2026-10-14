# crates/orchestration/peritus-scheduler/src/state/mutation — navigation

## Responsibility

Implementation pieces for `mutation.rs`: Reducer-only state mutation and derived invariant maintenance.

## Files in this folder

| File | Role |
| --- | --- |
| [acknowledge_start.rs](acknowledge_start.rs) | Atomic reducer mutation for dispatch start acknowledgement. |
| [cursor.rs](cursor.rs) | Exact event-cursor and state-digest mutations. |
| [dependency_action.rs](dependency_action.rs) | Exact dependency action selected by the production refresh scan. |
| [dependency_budget.rs](dependency_budget.rs) | Finite executable budget for total dependency propagation. |
| [dependency_round.rs](dependency_round.rs) | Exact ordered collection of dependency refresh changes. |
| [entity_insertion.rs](entity_insertion.rs) | Production worker and work insertion with actual-state preservation contracts. |
| [insertion_slots.rs](insertion_slots.rs) | Exact insertion positions for live and historical dispatch identities. |
| [reservation_command.rs](reservation_command.rs) | Production command kernels for live reservation transitions. |
| [reservation_remove.rs](reservation_remove.rs) | Exact live-reservation removal used by combined release transitions. |
| [reservation_update.rs](reservation_update.rs) | Reservation bookkeeping mutations. |
| [reserve.rs](reserve.rs) | Production mutation that installs one selected reservation. |
| [scheduler_phase.rs](scheduler_phase.rs) | Production scheduler lifecycle controls. |
| [set_terminal.rs](set_terminal.rs) | Exact terminal-state mutation. |
| [work_command.rs](work_command.rs) | Production command kernels for standalone work lifecycle changes. |
| [work_update.rs](work_update.rs) | Reservation-stable work lifecycle mutations. |
| [worker_refresh.rs](worker_refresh.rs) | Exact worker availability refresh after reservation mutations. |
| [worker_update.rs](worker_update.rs) | Reservation-stable worker lifecycle mutations. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [dependency_round/](dependency_round/AGENTS.md) | Implementation pieces for `dependency_round.rs`: Exact ordered collection of dependency refresh changes. |
| [entity_insertion/](entity_insertion/AGENTS.md) | Implementation pieces for `entity_insertion.rs`: Production worker and work insertion with actual-state preservation contracts. |
| [reservation_command/](reservation_command/AGENTS.md) | Implementation pieces for `reservation_command.rs`: Production command kernels for live reservation transitions. |
| [reserve/](reserve/AGENTS.md) | Implementation pieces for `reserve.rs`: Production mutation that installs one selected reservation. |
| [work_update/](work_update/AGENTS.md) | Implementation pieces for `work_update.rs`: Reservation-stable work lifecycle mutations. |
| [worker_refresh/](worker_refresh/AGENTS.md) | Implementation pieces for `worker_refresh.rs`: Exact worker availability refresh after reservation mutations. |

Owning module: [mutation.rs](../mutation.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
