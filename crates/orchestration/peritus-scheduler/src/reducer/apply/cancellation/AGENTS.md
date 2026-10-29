# crates/orchestration/peritus-scheduler/src/reducer/apply/cancellation — navigation

## Responsibility

Implementation pieces for `cancellation.rs`: Exact finite parent closure for production cancellation commands.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Exact root admission and event construction for production cancellation. |
| [ids.rs](ids.rs) | Canonical finite identity set used by cancellation traversal. |
| [update.rs](update.rs) | Exact cancellation lifecycle updates for a preselected work sequence. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [update/](update/AGENTS.md) | Implementation pieces for `update.rs`: Exact cancellation lifecycle updates for a preselected work sequence. |

Owning module: [cancellation.rs](../cancellation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
