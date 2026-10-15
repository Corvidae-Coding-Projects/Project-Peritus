# crates/app/peritus-app-protocol/src/subscription — navigation

## Responsibility

Bounded at-least-once event subscription values and state transitions.

## Files in this folder

| File | Role |
| --- | --- |
| [error.rs](error.rs) | Subscription-specific rejection vocabulary. |
| [frame.rs](frame.rs) | Exact registered B3 event frames retained for lossless redelivery. |
| [mod.rs](mod.rs) | Bounded at-least-once event subscription values and state transitions. |
| [state.rs](state.rs) | Pure subscription state and correlated transition values. |
| [transitions.rs](transitions.rs) | Subscription delivery, acknowledgement, flow-control, and terminal transitions. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `cursor_advances`, `acknowledgement_is_legal`, `delivery_window_is_safe`. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
