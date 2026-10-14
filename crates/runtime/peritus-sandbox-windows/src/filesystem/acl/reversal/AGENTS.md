# crates/runtime/peritus-sandbox-windows/src/filesystem/acl/reversal — navigation

## Responsibility

Implementation pieces for `reversal.rs`: Exact ACL installation, retained backup ownership, and retryable reversal.

## Files in this folder

| File | Role |
| --- | --- |
| [snapshots.rs](snapshots.rs) | Pristine DACL and retained-object inventory for the exact planned target subtrees. |

Owning module: [reversal.rs](../reversal.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
