# crates/app/peritus-daemon/src/startup/plan — navigation

## Responsibility

Implementation pieces for `plan.rs`: Pure startup checkpoint sequencing and deterministic kill/restart failpoints.

## Files in this folder

| File | Role |
| --- | --- |
| [checkpoint.rs](checkpoint.rs) | Fixed-capacity startup checkpoint observations. |
| [failpoint.rs](failpoint.rs) | Fixed-size failpoint vocabulary for startup kill/restart testing. |

Owning module: [plan.rs](../plan.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
