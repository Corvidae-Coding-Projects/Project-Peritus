# crates/app/peritus-launcher/src/workspace_setup — navigation

## Responsibility

Implementation pieces for `workspace_setup.rs`: Ergonomic repository selection, explicit trust, recent choices, and repair.

## Files in this folder

| File | Role |
| --- | --- |
| [discovery.rs](discovery.rs) | Friendly path handling over the structured C1 Git discovery boundary. |
| [managed.rs](managed.rs) | Application-managed writable worktree publication and health inspection. |

Owning module: [workspace_setup.rs](../workspace_setup.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
