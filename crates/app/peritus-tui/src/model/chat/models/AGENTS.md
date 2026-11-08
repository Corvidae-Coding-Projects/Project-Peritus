# crates/app/peritus-tui/src/model/chat/models — navigation

## Responsibility

Implementation pieces for `models.rs`: Exact provider-discovered selection; manual identifiers are always explicitly marked.

## Files in this folder

| File | Role |
| --- | --- |
| [effort.rs](effort.rs) | Discoverable effort picker and exact per-role command handling. |
| [direct.rs](direct.rs) | Atomic model/effort commands with automatic catalog loading, retained drafts, and destination-bound response handling. |
| [wizard.rs](wizard.rs) | Guided agent, model, and effort selection with staged choices, Back/Cancel, and one destination-bound final save. |

Owning module: [models.rs](../models.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
