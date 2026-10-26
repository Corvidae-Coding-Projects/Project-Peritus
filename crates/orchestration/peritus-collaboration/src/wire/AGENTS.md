# crates/orchestration/peritus-collaboration/src/wire — navigation

## Responsibility

Collaboration-owned typed B3 codecs for canonical families 73, 74, and 75.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Canonical family-73 collaboration command codec. |
| [event.rs](event.rs) | Canonical family-74 collaboration event codec. |
| [mod.rs](mod.rs) | Collaboration-owned typed B3 codecs for canonical families 73, 74, and 75. |
| [state.rs](state.rs) | Canonical family-75 complete collaboration-state codec. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
