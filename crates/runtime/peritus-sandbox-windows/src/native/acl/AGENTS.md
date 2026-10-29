# crates/runtime/peritus-sandbox-windows/src/native/acl — navigation

## Responsibility

Implementation pieces for `acl.rs`: Exact DACL backup ownership on retained filesystem object handles.

## Files in this folder

| File | Role |
| --- | --- |
| [anchor.rs](anchor.rs) | Atomically create and retain an owned deny directory, with no open-after-create race. |
| [descriptor.rs](descriptor.rs) | DACL-only backup/restore; owner and SACL are never changed. |
| [inheritance.rs](inheritance.rs) | Detect temporary principal ACEs left on newly created objects without editing caller ACLs. |
| [reservation.rs](reservation.rs) | Cross-process, movable ACL exclusion for each complete filesystem volume. |
| [stable_path.rs](stable_path.rs) | Transient path stabilization for the remaining path-based icacls apply operation. |

Owning module: [acl.rs](../acl.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
