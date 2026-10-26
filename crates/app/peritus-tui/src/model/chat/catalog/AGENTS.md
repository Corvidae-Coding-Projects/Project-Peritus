# crates/app/peritus-tui/src/model/chat/catalog — navigation

## Responsibility

Implementation pieces for `catalog.rs`: Typed local command metadata. Only directly entered commands are executable intents.

## Files in this folder

| File | Role |
| --- | --- |
| [entries.rs](entries.rs) | Explicit command catalog entries, with no source-generating macros. |

Owning module: [catalog.rs](../catalog.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
