# crates/app/peritus-app-protocol/src/workbench/memory — navigation

## Responsibility

Implementation pieces for `memory.rs`: Explicit project-local guidance, exact provenance and deterministic future-request rendering.

## Files in this folder

| File | Role |
| --- | --- |
| [inspection.rs](inspection.rs) | Revision-fenced bounded inspection queries, rows, and pages. |
| [lifecycle.rs](lifecycle.rs) | Active guidance records and content-free durable tombstones. |
| [render.rs](render.rs) | Scope-filtered deterministic future-request guidance rendering. |
| [transition.rs](transition.rs) | Pure revision-fenced project-guidance lifecycle transitions. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [transition/](transition/AGENTS.md) | Implementation pieces for `transition.rs`: Pure revision-fenced project-guidance lifecycle transitions. |

Owning module: [memory.rs](../memory.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
