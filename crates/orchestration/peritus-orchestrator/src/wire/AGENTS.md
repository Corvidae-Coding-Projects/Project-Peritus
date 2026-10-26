# crates/orchestration/peritus-orchestrator/src/wire — navigation

## Responsibility

Orchestrator-owned canonical families 76, 77, and 78.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Canonical family-76 E0 command codec. |
| [event.rs](event.rs) | Canonical family-77 E0 event codec. |
| [mod.rs](mod.rs) | Orchestrator-owned canonical families 76, 77, and 78. |
| [state.rs](state.rs) | Canonical family-78 complete E0 checkpoint codec. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
