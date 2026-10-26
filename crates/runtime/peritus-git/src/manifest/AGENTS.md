# crates/runtime/peritus-git/src/manifest — navigation

## Responsibility

Implementation pieces for `manifest.rs`: Canonical, versioned persistence records for restart-safe Git handles.

## Files in this folder

| File | Role |
| --- | --- |
| [candidate.rs](candidate.rs) | Candidate-tree manifest representation. |
| [snapshot.rs](snapshot.rs) | Retained-snapshot manifest representation and restart reopening. |
| [worktree.rs](worktree.rs) | Registered-worktree manifest representation and restart recovery. |

Owning module: [manifest.rs](../manifest.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
