# crates/orchestration/peritus-scheduler/src/state/mutation/reservation_command — navigation

## Responsibility

Implementation pieces for `reservation_command.rs`: Production command kernels for live reservation transitions.

## Files in this folder

| File | Role |
| --- | --- |
| [acknowledge_cancellation.rs](acknowledge_cancellation.rs) | Cancellation acknowledgement through the exact reservation release kernel. |
| [complete.rs](complete.rs) | Successful completion through the exact reservation release kernel. |
| [outcome.rs](outcome.rs) | Deterministic outcomes from production reservation command kernels. |

Owning module: [reservation_command.rs](../reservation_command.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
