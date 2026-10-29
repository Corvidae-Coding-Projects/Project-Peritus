# crates/orchestration/peritus-scheduler/src/reducer — navigation

## Responsibility

Implementation pieces for `reducer.rs`: Pure deterministic scheduler reduction and exact replay.

## Files in this folder

| File | Role |
| --- | --- |
| [apply.rs](apply.rs) | Closed command application against a cloned scheduler state. |
| [decision.rs](decision.rs) | Exact cursor and event commitment around ordinary successor hashing. |
| [fences.rs](fences.rs) | Exact command-fence admission before semantic reduction. |
| [reconstruction.rs](reconstruction.rs) | Exact causative-command reconstruction used by production event replay. |
| [start.rs](start.rs) | Exact genesis preparation and event commitment around ordinary hashing. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [apply/](apply/AGENTS.md) | Implementation pieces for `apply.rs`: Closed command application against a cloned scheduler state. |

Owning module: [reducer.rs](../reducer.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
