# crates/runtime/peritus-git/src/worktree — navigation

## Responsibility

Implementation pieces for `worktree.rs`: Detached linked-worktree registrations and lifecycle operations.

## Files in this folder

| File | Role |
| --- | --- |
| [lifecycle.rs](lifecycle.rs) | Validated creation, observation, and exact removal effects. |
| [recovery.rs](recovery.rs) | Recovery of a Git-created worktree whose typed handle was not persisted. |

Owning module: [worktree.rs](../worktree.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
