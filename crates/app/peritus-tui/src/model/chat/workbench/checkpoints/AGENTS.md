# crates/app/peritus-tui/src/model/chat/workbench/checkpoints — navigation

## Responsibility

Implementation pieces for `checkpoints.rs`: Explicit bounded checkpoint creation and preview-bound rewind confirmation.

## Files in this folder

| File | Role |
| --- | --- |
| [commands.rs](commands.rs) | Checkpoint creation and rewind request commands. |
| [pages.rs](pages.rs) | Lazy checkpoint coverage navigation. |
| [receipts.rs](receipts.rs) | Exact correlated checkpoint and restore responses. |

Owning module: [checkpoints.rs](../checkpoints.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
