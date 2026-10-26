# crates/analysis/peritus-evolution/src/pointer — navigation

## Responsibility

Long-lived project production-pointer aggregate and append-only rollback.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Closed CAS-fenced production-pointer commands. |
| [event.rs](event.rs) | Accepted production-pointer events and successor checkpoints. |
| [mod.rs](mod.rs) | Long-lived project production-pointer aggregate and append-only rollback. |
| [projection.rs](projection.rs) | Rebuildable query projection for the production-harness pointer. |
| [reducer.rs](reducer.rs) | Pure production-pointer decision, event application, and replay. |
| [rollback.rs](rollback.rs) | Exact authorization boundary facts and auditable rollback proposals. |
| [state.rs](state.rs) | Complete long-lived project production-pointer checkpoint. |

## Working in this area

Start at [`mod.rs`](mod.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
