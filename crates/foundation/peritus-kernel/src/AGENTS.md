# crates/foundation/peritus-kernel/src — navigation

## Responsibility

Lifecycle state machines and authoritative event reduction for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [aggregate.rs](aggregate.rs) | Complete lifecycle aggregate and genesis transition. |
| [authorization.rs](authorization.rs) | Checked projection of an exact B1 capability-use transition. |
| [command.rs](command.rs) | Exhaustive lifecycle command vocabulary. |
| [envelope.rs](envelope.rs) | Exact command/event identity and causal binding. |
| [error.rs](error.rs) | Stable reducer failures. |
| [event.rs](event.rs) | Logical event plans emitted by accepted commands. |
| [identity.rs](identity.rs) | Extensional equality for opaque lifecycle identities. |
| [inputs.rs](inputs.rs) | Borrowed verified facts used by reducer commands. |
| [lib.rs](lib.rs) | Lifecycle state machines and authoritative event reduction for Peritus. |
| [reducer.rs](reducer.rs) | Total authoritative lifecycle reducer. |
| [state.rs](state.rs) | Closed lifecycle phases and checked state records. |
| [transition.rs](transition.rs) | Applied and rejected reducer outcomes. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [aggregate/](aggregate/AGENTS.md) | Implementation pieces for `aggregate.rs`: Complete lifecycle aggregate and genesis transition. |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Total authoritative lifecycle reducer. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Closed lifecycle phases and checked state records. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
