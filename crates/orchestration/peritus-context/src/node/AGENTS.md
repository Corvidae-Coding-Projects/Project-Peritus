# crates/orchestration/peritus-context/src/node — navigation

## Responsibility

Implementation pieces for `node.rs`: Checked context nodes and canonical role/dependency metadata.

## Files in this folder

| File | Role |
| --- | --- |
| [metadata.rs](metadata.rs) | Exact context-node metadata admission and semantic cloning. |
| [visibility.rs](visibility.rs) | Exact admission and lookup for node role visibility. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [metadata/](metadata/AGENTS.md) | Implementation pieces for `metadata.rs`: Exact context-node metadata admission and semantic cloning. |

Owning module: [node.rs](../node.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
