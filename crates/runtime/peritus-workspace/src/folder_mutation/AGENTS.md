# crates/runtime/peritus-workspace/src/folder_mutation — navigation

## Responsibility

Implementation pieces for `folder_mutation.rs`: Exact-authority patch application for one registered ordinary folder.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Registered-folder identity, revision, and canonical action-payload binding. |
| [outcome.rs](outcome.rs) | Read-only exact folder mutation outcome projection. |
| [recovery.rs](recovery.rs) | Exact-authority restart recovery for one registered-folder patch. |

Owning module: [folder_mutation.rs](../folder_mutation.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
