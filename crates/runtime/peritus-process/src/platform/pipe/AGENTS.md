# crates/runtime/peritus-process/src/platform/pipe — navigation

## Responsibility

Implementation pieces for `pipe.rs`: Direct structured pipe process launch with process-tree containment.

## Files in this folder

| File | Role |
| --- | --- |
| [windows_reap.rs](windows_reap.rs) | Bounded Windows job-object completion ownership. |

Owning module: [pipe.rs](../pipe.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
