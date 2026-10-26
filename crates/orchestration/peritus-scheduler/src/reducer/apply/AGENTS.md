# crates/orchestration/peritus-scheduler/src/reducer/apply — navigation

## Responsibility

Implementation pieces for `apply.rs`: Closed command application against a cloned scheduler state.

## Files in this folder

| File | Role |
| --- | --- |
| [admission.rs](admission.rs) | Production work admission with exact rejection and successor contracts. |
| [cancellation.rs](cancellation.rs) | Exact finite parent closure for production cancellation commands. |
| [control.rs](control.rs) | Cancellation, pause/drain, abandonment, exhaustion, and terminal control. |
| [dispatch.rs](dispatch.rs) | Reservation mutation selected by the production dispatch command. |
| [loss.rs](loss.rs) | Exact worker-loss selection and classification over pre-transition records. |
| [worker_control.rs](worker_control.rs) | Worker registration, availability, draining, and removal. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [admission/](admission/AGENTS.md) | Implementation pieces for `admission.rs`: Production work admission with exact rejection and successor contracts. |
| [cancellation/](cancellation/AGENTS.md) | Implementation pieces for `cancellation.rs`: Exact finite parent closure for production cancellation commands. |
| [control/](control/AGENTS.md) | Implementation pieces for `control.rs`: Cancellation, pause/drain, abandonment, exhaustion, and terminal control. |
| [loss/](loss/AGENTS.md) | Implementation pieces for `loss.rs`: Exact worker-loss selection and classification over pre-transition records. |

Owning module: [apply.rs](../apply.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
