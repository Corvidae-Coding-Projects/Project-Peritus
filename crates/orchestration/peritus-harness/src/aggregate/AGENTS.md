# crates/orchestration/peritus-harness/src/aggregate — navigation

## Responsibility

Pure harness command, event, state, and reducer model.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Closed administrative harness command vocabulary. |
| [error.rs](error.rs) | Typed pure aggregate and replay failures. |
| [event.rs](event.rs) | Accepted semantic events and exact successor-state transitions. |
| [mod.rs](mod.rs) | Pure harness command, event, state, and reducer model. |
| [reducer.rs](reducer.rs) | Pure checked command decision and event replay reducer. |
| [state.rs](state.rs) | Complete bounded harness aggregate state and canonical checkpoint data. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Pure checked command decision and event replay reducer. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
