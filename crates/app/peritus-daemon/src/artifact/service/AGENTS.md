# crates/app/peritus-daemon/src/artifact/service — navigation

## Responsibility

Implementation pieces for `service.rs`: Serialized transfer registry co-owned with the journal and artifact catalog.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Typed error translation for the artifact authority boundary. |
| [scoped.rs](scoped.rs) | Scoped workbench imports reuse the existing bounded upload and immutable read machinery. |

Owning module: [service.rs](../service.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
