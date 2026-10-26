# crates/state/peritus-artifact-store/src/catalog — navigation

## Responsibility

Implementation pieces for `catalog.rs`: SQLite-backed durable artifact metadata and references.

## Files in this folder

| File | Role |
| --- | --- |
| [schema.rs](schema.rs) | Initial durable artifact catalog schema. |
| [value.rs](value.rs) | Validation and conversion of stored artifact values. |

Owning module: [catalog.rs](../catalog.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
