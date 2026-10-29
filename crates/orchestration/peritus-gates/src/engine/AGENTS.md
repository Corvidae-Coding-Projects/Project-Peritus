# crates/orchestration/peritus-gates/src/engine — navigation

## Responsibility

Implementation pieces for `engine.rs`: Thin effect shell around the pure reducer and atomic durability boundary.

## Files in this folder

| File | Role |
| --- | --- |
| [publication.rs](publication.rs) | Evidence publication from an exact durable passing-result observation. |

Owning module: [engine.rs](../engine.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
