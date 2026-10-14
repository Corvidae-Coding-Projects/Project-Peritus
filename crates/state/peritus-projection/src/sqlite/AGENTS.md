# crates/state/peritus-projection/src/sqlite — navigation

## Responsibility

Narrow `SQLite` adapter for shared-file projection generations.

## Files in this folder

| File | Role |
| --- | --- |
| [mod.rs](mod.rs) | Narrow `SQLite` adapter for shared-file projection generations. |
| [schema.rs](schema.rs) | Stable projection-owned `SQLite` schema. |
| [store.rs](store.rs) | Connection policy, generation reads, and startup repair planning. |
| [swap.rs](swap.rs) | Transactional shadow-generation install and active-pointer swap. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
