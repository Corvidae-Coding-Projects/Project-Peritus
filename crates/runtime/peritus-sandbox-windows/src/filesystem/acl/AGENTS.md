# crates/runtime/peritus-sandbox-windows/src/filesystem/acl — navigation

## Responsibility

Implementation pieces for `acl.rs`: Canonical ACL entries and exact save/apply/restore transaction.

## Files in this folder

| File | Role |
| --- | --- |
| [reversal.rs](reversal.rs) | Exact ACL installation, retained backup ownership, and retryable reversal. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [reversal/](reversal/AGENTS.md) | Implementation pieces for `reversal.rs`: Exact ACL installation, retained backup ownership, and retryable reversal. |

Owning module: [acl.rs](../acl.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
