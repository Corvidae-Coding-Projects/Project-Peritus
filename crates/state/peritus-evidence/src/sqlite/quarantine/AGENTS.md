# crates/state/peritus-evidence/src/sqlite/quarantine — navigation

## Responsibility

Implementation pieces for `quarantine.rs`: Immutable audit observations and exact identity fences for quarantined evidence.

## Files in this folder

| File | Role |
| --- | --- |
| [query.rs](query.rs) | Read-only quarantine projections and deterministic audit-identity paging. |
| [recovery.rs](recovery.rs) | Exact-identity dependency reconciliation and reconstruction of quarantined records. |

Owning module: [quarantine.rs](../quarantine.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
