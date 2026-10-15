# crates/state/peritus-artifact-store/src — navigation

## Responsibility

Streaming content-addressed artifact storage for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [catalog.rs](catalog.rs) | SQLite-backed durable artifact metadata and references. |
| [config.rs](config.rs) | Store configuration validation. |
| [digest.rs](digest.rs) | Artifact digest values and canonical lowercase encoding. |
| [error.rs](error.rs) | Stable artifact-store errors and recovery guidance. |
| [finalize.rs](finalize.rs) | Narrow file hashing, verification, and atomic no-replace publication. |
| [gc_plan.rs](gc_plan.rs) | Deterministic mark, quarantine, restore, and sweep plans. |
| [lib.rs](lib.rs) | Streaming content-addressed artifact storage for Peritus. |
| [metadata.rs](metadata.rs) | Durable artifact metadata value types. |
| [owned.rs](owned.rs) | Owned streaming handles for long-lived daemon transfers. |
| [path.rs](path.rs) | Private fixed-layout path derivation and directory synchronization. |
| [quota.rs](quota.rs) | Checked quota accounting plans. |
| [recovery.rs](recovery.rs) | Idempotent restart recovery for temporary, object, quarantine, and catalog state. |
| [references.rs](references.rs) | Canonically ordered artifact reference sets. |
| [sqlite_interop.rs](sqlite_interop.rs) | Shared SQLite schema and transaction operations. |
| [store.rs](store.rs) | High-level store owner and narrow filesystem/catalog orchestration. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `checked_quota_totals`, `sweep_is_later`, `write_bounds_valid`. |
| [writer.rs](writer.rs) | Bounded streaming artifact writer. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [catalog/](catalog/AGENTS.md) | Implementation pieces for `catalog.rs`: SQLite-backed durable artifact metadata and references. |
| [store/](store/AGENTS.md) | Implementation pieces for `store.rs`: High-level store owner and narrow filesystem/catalog orchestration. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
