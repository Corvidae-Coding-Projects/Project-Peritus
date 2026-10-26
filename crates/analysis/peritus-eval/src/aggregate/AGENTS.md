# crates/analysis/peritus-eval/src/aggregate — navigation

## Responsibility

Pure durable evaluation campaign aggregate.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Closed CAS-fenced evaluation command vocabulary. |
| [event.rs](event.rs) | Accepted evaluation events and successor-state transitions. |
| [mod.rs](mod.rs) | Pure durable evaluation campaign aggregate. |
| [reducer.rs](reducer.rs) | Pure evaluation command decision, event application, and replay. |
| [state.rs](state.rs) | Complete compact family-87 evaluation checkpoint. |
| [types.rs](types.rs) | Compact durable campaign value records. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
