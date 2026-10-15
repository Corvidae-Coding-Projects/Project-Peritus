# crates/app/peritus-product-state/src/workspace — navigation

## Responsibility

Implementation pieces for `workspace.rs`: Durable workspace choices and remembered trust without live filesystem claims.

## Files in this folder

| File | Role |
| --- | --- |
| [profile.rs](profile.rs) | One remembered repository and its optional managed-worktree registration. |
| [selection.rs](selection.rs) | Most-recent-first workspace inventory and active selection. |

Owning module: [workspace.rs](../workspace.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
