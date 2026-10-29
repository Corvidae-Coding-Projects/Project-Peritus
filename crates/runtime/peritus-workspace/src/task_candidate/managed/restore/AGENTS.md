# crates/runtime/peritus-workspace/src/task_candidate/managed/restore — navigation

## Responsibility

Implementation pieces for `restore.rs`: Restore task preimages without changing unrelated staged or unstaged files.

## Files in this folder

| File | Role |
| --- | --- |
| [prepared.rs](prepared.rs) | Lock and construct every affected Git index before changing any workspace bytes. |
| [sources.rs](sources.rs) | Materialize all source preimages before source, index, HEAD, or archive publication. |

Owning module: [restore.rs](../restore.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
