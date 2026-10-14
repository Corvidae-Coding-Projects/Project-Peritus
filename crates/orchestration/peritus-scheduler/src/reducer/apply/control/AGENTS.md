# crates/orchestration/peritus-scheduler/src/reducer/apply/control — navigation

## Responsibility

Implementation pieces for `control.rs`: Cancellation, pause/drain, abandonment, exhaustion, and terminal control.

## Files in this folder

| File | Role |
| --- | --- |
| [finalization.rs](finalization.rs) | Exact finalization admission, summary planning, and terminal commit. |
| [phase.rs](phase.rs) | Exact phase-event construction before diagnostic error mapping. |

Owning module: [control.rs](../control.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
