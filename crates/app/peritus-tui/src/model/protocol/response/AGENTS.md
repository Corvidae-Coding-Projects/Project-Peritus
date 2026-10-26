# crates/app/peritus-tui/src/model/protocol/response — navigation

## Responsibility

Implementation pieces for `response.rs`: Request-correlated response projection and retained draft recovery.

## Files in this folder

| File | Role |
| --- | --- |
| [control.rs](control.rs) | Correlated workbench and diagnostic response projection. |

Owning module: [response.rs](../response.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
