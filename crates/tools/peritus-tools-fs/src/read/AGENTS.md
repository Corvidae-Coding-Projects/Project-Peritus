# crates/tools/peritus-tools-fs/src/read — navigation

## Responsibility

Implementation pieces for `read.rs`: Bounded immutable filesystem observations.

## Files in this folder

| File | Role |
| --- | --- |
| [search.rs](search.rs) | Literal filesystem search with exact global continuation offsets. |
| [traversal.rs](traversal.rs) | Incremental deterministic traversal over checked C1 directory pages. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [search/](search/AGENTS.md) | Implementation pieces for `search.rs`: Literal filesystem search with exact global continuation offsets. |

Owning module: [read.rs](../read.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
