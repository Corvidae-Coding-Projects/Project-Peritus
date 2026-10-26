# crates/state/peritus-migrations/src — navigation

## Responsibility

Forward-only ordered migrations, backups, and recovery for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [backup.rs](backup.rs) | Consistent `SQLite` backup, digest verification, synchronization, and restoration. |
| [catalog.rs](catalog.rs) | Migration-owned `SQLite` schema and typed durable queries. |
| [config.rs](config.rs) | Migration engine configuration and application compatibility. |
| [descriptor.rs](descriptor.rs) | Immutable migration descriptor values. |
| [engine.rs](engine.rs) | Single-owner migration engine and forward-only apply state machine. |
| [error.rs](error.rs) | Stable migration errors and recovery classifications. |
| [lib.rs](lib.rs) | Forward-only ordered migrations, backups, and recovery for Peritus. |
| [plan.rs](plan.rs) | Deterministic migration planning. |
| [preflight.rs](preflight.rs) | Preflight integrity and capacity observations. |
| [recovery.rs](recovery.rs) | Durable migration-operation recovery values. |
| [registry.rs](registry.rs) | Initial release schema marker and future forward-migration registry. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `versions_are_contiguous`, `versions_are_compatible`, `backup_required`, `checked_required_space`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [engine/](engine/AGENTS.md) | Implementation pieces for `engine.rs`: Single-owner migration engine and forward-only apply state machine. |
| [registry/](registry/AGENTS.md) | Implementation pieces for `registry.rs`: Initial release schema marker and future forward-migration registry. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
