# crates/app/peritus-daemon/src/product_run/workbench/launch — navigation

## Responsibility

Implementation pieces for `launch.rs`: Durable admission and daemon-owned preview launch, interaction, capture, and inspection.

## Files in this folder

| File | Role |
| --- | --- |
| [aggregate.rs](aggregate.rs) | Durable preview aggregate admission and immutable result rebuilding. |
| [capture.rs](capture.rs) | Selected-window capture, artifact publication, and host capability discovery. |
| [evidence.rs](evidence.rs) | Preview binding checks and graphical-goal evidence qualification. |
| [output.rs](output.rs) | Scoped, bounded, restart-retained preview output projection. |
| [process.rs](process.rs) | Owned preview process launch, interaction, observation, and termination. |
| [terminal.rs](terminal.rs) | Authenticated terminal access to the exact conversation-owned preview process. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [capture/](capture/AGENTS.md) | Implementation pieces for `capture.rs`: Selected-window capture, artifact publication, and host capability discovery. |
| [evidence/](evidence/AGENTS.md) | Implementation pieces for `evidence.rs`: Preview binding checks and graphical-goal evidence qualification. |

Owning module: [launch.rs](../launch.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
