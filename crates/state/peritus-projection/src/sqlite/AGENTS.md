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

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
