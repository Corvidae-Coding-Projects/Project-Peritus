# crates/orchestration/peritus-run-knowledge/src/delta — navigation

## Responsibility

Implementation pieces for `delta.rs`: Role-specific changed-fact and current-reference packet planning.

## Files in this folder

| File | Role |
| --- | --- |
| [material.rs](material.rs) | Complete executable material comparisons for delta-packet reuse. |
| [packet.rs](packet.rs) | Delta-packet values, exact field views, and semantic cloning. |

Owning module: [delta.rs](../delta.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
