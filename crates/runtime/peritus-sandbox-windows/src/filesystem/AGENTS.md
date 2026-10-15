# crates/runtime/peritus-sandbox-windows/src/filesystem — navigation

## Responsibility

Implementation pieces for `filesystem.rs`: Windows path identity, reparse checks, and exact temporary ACL policy.

## Files in this folder

| File | Role |
| --- | --- |
| [acl.rs](acl.rs) | Canonical ACL entries and exact save/apply/restore transaction. |
| [path.rs](path.rs) | Lexical Windows path normalization and native reparse evidence. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [acl/](acl/AGENTS.md) | Implementation pieces for `acl.rs`: Canonical ACL entries and exact save/apply/restore transaction. |
| [path/](path/AGENTS.md) | Implementation pieces for `path.rs`: Lexical Windows path normalization and native reparse evidence. |

Owning module: [filesystem.rs](../filesystem.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
