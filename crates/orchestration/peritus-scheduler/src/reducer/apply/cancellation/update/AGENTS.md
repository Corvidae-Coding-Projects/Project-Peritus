# crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation/update — navigation

## Responsibility

Implementation pieces for `update.rs`: Exact cancellation lifecycle updates for a preselected work sequence.

## Files in this folder

| File | Role |
| --- | --- |
| [activity.rs](activity.rs) | Reservation membership scan used by one cancellation update. |

Owning module: [update.rs](../update.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
