# crates/state/peritus-journal/src/sqlite/query — navigation

## Responsibility

Implementation pieces for `query.rs`: Exact immutable query and command-resolution paths.

## Files in this folder

| File | Role |
| --- | --- |
| [aggregate.rs](aggregate.rs) | Aggregate-head and complete aggregate-chain reads. |
| [command.rs](command.rs) | Idempotent command resolution and committed-batch reconstruction. |
| [records.rs](records.rs) | Immutable event-range loading and exact row validation. |
| [state.rs](state.rs) | Current and historical journal-owned state reads. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [aggregate/](aggregate/AGENTS.md) | Implementation pieces for `aggregate.rs`: Aggregate-head and complete aggregate-chain reads. |

Owning module: [query.rs](../query.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
