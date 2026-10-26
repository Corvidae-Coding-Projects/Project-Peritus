# crates/runtime/peritus-workspace/src/task_candidate/managed/retention — navigation

## Responsibility

Implementation pieces for `retention.rs`: Independently retained nested Git history and metadata in the enclosing workspace.

## Files in this folder

| File | Role |
| --- | --- |
| [linked.rs](linked.rs) | Preserve linked worktree identity without copying or relocating its shared Git database. |
| [metadata.rs](metadata.rs) | Preserve original Git metadata as immutable blobs, without following metadata symlinks. |
| [restore.rs](restore.rs) | Reconstruct deleted original repositories privately before discard touches current source. |
| [store.rs](store.rs) | Local Git fetch shares immutable objects across task baselines without copying full packs. |

Owning module: [retention.rs](../retention.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
