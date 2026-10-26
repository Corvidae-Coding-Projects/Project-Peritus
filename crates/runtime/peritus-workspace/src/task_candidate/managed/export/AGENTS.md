# crates/runtime/peritus-workspace/src/task_candidate/managed/export — navigation

## Responsibility

Implementation pieces for `export.rs`: Exact source patches across existing and task-created repositories.

## Files in this folder

| File | Role |
| --- | --- |
| [instructions.rs](instructions.rs) | Deterministic ordinary-Git source consumption with each repository's own index. |
| [submodule.rs](submodule.rs) | Recover export preimages for a recorded submodule initialized during the task. |

Owning module: [export.rs](../export.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
