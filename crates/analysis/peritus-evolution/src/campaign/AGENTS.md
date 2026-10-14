# crates/analysis/peritus-evolution/src/campaign — navigation

## Responsibility

Immutable evolution-campaign aggregate and exact promotion proposals.

## Files in this folder

| File | Role |
| --- | --- |
| [command.rs](command.rs) | Closed CAS-fenced evolution-campaign commands. |
| [event.rs](event.rs) | Accepted campaign semantic events and successor checkpoints. |
| [mod.rs](mod.rs) | Immutable evolution-campaign aggregate and exact promotion proposals. |
| [projection.rs](projection.rs) | Rebuildable read model for one evolution campaign. |
| [proposal.rs](proposal.rs) | Exact inert promotion and publication records. |
| [reducer.rs](reducer.rs) | Pure campaign decision, event application, and replay. |
| [state.rs](state.rs) | Complete pure evolution-campaign checkpoint. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Pure campaign decision, event application, and replay. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
