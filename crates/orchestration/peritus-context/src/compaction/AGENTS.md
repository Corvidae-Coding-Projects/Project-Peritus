# crates/orchestration/peritus-context/src/compaction — navigation

## Responsibility

Implementation pieces for `compaction.rs`: Checked compaction proposals that preserve source ranges and complete lineage.

## Files in this folder

| File | Role |
| --- | --- |
| [access.rs](access.rs) | Stable accessors for checked compaction evidence. |
| [replacement.rs](replacement.rs) | Atomic graph replacement for already validated compaction evidence. |
| [validation.rs](validation.rs) | Transactional validation and construction of compacted nodes. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [replacement/](replacement/AGENTS.md) | Implementation pieces for `replacement.rs`: Atomic graph replacement for already validated compaction evidence. |

Owning module: [compaction.rs](../compaction.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
