# crates/runtime/peritus-process/src/platform/pty — navigation

## Responsibility

Implementation pieces for `pty.rs`: Direct structured PTY process launch and session ownership.

## Files in this folder

| File | Role |
| --- | --- |
| [windows.rs](windows.rs) | Native `ConPTY` launch with suspended-process Job Object admission. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [windows/](windows/AGENTS.md) | Implementation pieces for `windows.rs`: Native `ConPTY` launch with suspended-process Job Object admission. |

Owning module: [pty.rs](../pty.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
