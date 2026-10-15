# crates/state/peritus-evidence/src/sqlite — navigation

## Responsibility

Narrow shared-file `SQLite` evidence adapter.

## Files in this folder

| File | Role |
| --- | --- |
| [connection.rs](connection.rs) | Hardened shared-database connection policy. |
| [containment.rs](containment.rs) | Durable keyset startup progress, read snapshots, and per-record containment transactions. |
| [mod.rs](mod.rs) | Narrow shared-file `SQLite` evidence adapter. |
| [quarantine.rs](quarantine.rs) | Immutable audit observations and exact identity fences for quarantined evidence. |
| [row.rs](row.rs) | Durable row parsing and journal observations. |
| [schema.rs](schema.rs) | Stable evidence-owned `SQLite` schema and atomic recovery migration. |
| [store.rs](store.rs) | Evidence connection policy, atomic admission, reads, and invalidation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [quarantine/](quarantine/AGENTS.md) | Implementation pieces for `quarantine.rs`: Immutable audit observations and exact identity fences for quarantined evidence. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
