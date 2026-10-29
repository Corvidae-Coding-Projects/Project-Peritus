# crates/orchestration/peritus-scheduler/src/state/mutation/work_update — navigation

## Responsibility

Implementation pieces for `work_update.rs`: Reservation-stable work lifecycle mutations.

## Files in this folder

| File | Role |
| --- | --- |
| [apply_at.rs](apply_at.rs) | Exact lifecycle mutation at an already-validated retained work index. |
| [attempt.rs](attempt.rs) | Exact work-attempt mutation used by reservation admission. |
| [release.rs](release.rs) | Combined reservation release and work lifecycle transitions. |
| [wrappers.rs](wrappers.rs) | Public phase-update operations over the verified work mutation kernel. |

Owning module: [work_update.rs](../work_update.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
