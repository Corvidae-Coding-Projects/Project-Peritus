# crates/analysis/peritus-debugger/src/aggregate — navigation

## Responsibility

Pure durable debugger job state machine.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Closed, fenced debugger command vocabulary. |
| [event.rs](event.rs) | Accepted debugger events and exact successor-state transitions. |
| [mod.rs](mod.rs) | Pure durable debugger job state machine. |
| [reducer.rs](reducer.rs) | Pure command decision and deterministic event replay. |
| [state.rs](state.rs) | Complete bounded debugger aggregate checkpoint. |
| [types.rs](types.rs) | Bounded values retained by the debugger aggregate. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [command/](command/AGENTS.md) | Implementation pieces for `command.rs`: Closed, fenced debugger command vocabulary. |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Pure command decision and deterministic event replay. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Complete bounded debugger aggregate checkpoint. |
| [types/](types/AGENTS.md) | Implementation pieces for `types.rs`: Bounded values retained by the debugger aggregate. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
