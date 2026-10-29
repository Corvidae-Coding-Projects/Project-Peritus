# crates/orchestration/peritus-context/src/node/metadata — navigation

## Responsibility

Implementation pieces for `metadata.rs`: Exact context-node metadata admission and semantic cloning.

## Files in this folder

| File | Role |
| --- | --- |
| [model.rs](model.rs) | Recursive specification of metadata admission and failure priority. |
| [validation.rs](validation.rs) | Production traversal for canonical metadata dependencies. |

Owning module: [metadata.rs](../metadata.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
