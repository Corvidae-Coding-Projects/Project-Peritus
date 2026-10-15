# crates/app/peritus-tui/src/title — navigation

## Responsibility

Implementation pieces for `title.rs`: Scoped ownership of the Windows console title shared with foreground children.

## Files in this folder

| File | Role |
| --- | --- |
| [windows.rs](windows.rs) | Exact UTF-16 console-title capture and restoration at the G2 presentation boundary. |

Owning module: [title.rs](../title.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
