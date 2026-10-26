# crates/orchestration/peritus-gates/src — navigation

## Responsibility

Durable deterministic acceptance gate orchestration for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [canonical.rs](canonical.rs) | Domain-canonical D1 state and terminal hashing. |
| [command.rs](command.rs) | Closed pure D1 command vocabulary. |
| [descriptor.rs](descriptor.rs) | Exact acceptance-contract to C4 quality-definition planning. |
| [durability.rs](durability.rs) | Atomic C0 persistence and checked replay loading for one D1 aggregate. |
| [engine.rs](engine.rs) | Thin effect shell around the pure reducer and atomic durability boundary. |
| [error.rs](error.rs) | Stable D1 rejection, error, and recovery vocabulary. |
| [event.rs](event.rs) | Immutable D1 events and reducer transitions. |
| [evidence.rs](evidence.rs) | Exact-revision gate evidence publication contracts. |
| [lib.rs](lib.rs) | Durable deterministic acceptance gate orchestration for Peritus. |
| [obligation.rs](obligation.rs) | Pure D1 bridge from typed public obligations to gate qualification. |
| [outcome.rs](outcome.rs) | Closed quality-attempt results used by the pure D1 reducer. |
| [projection.rs](projection.rs) | Rebuildable read-only D1 projections. |
| [reducer.rs](reducer.rs) | Pure deterministic D1 transition, aggregation, and replay. |
| [state.rs](state.rs) | Complete authoritative D1 run state. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `attempts_are_bounded`, `dependencies_are_satisfied`, `dependency_order_is_legal`, `evidence_is_fresh`, `terminal_truthful`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [durability/](durability/AGENTS.md) | Implementation pieces for `durability.rs`: Atomic C0 persistence and checked replay loading for one D1 aggregate. |
| [engine/](engine/AGENTS.md) | Implementation pieces for `engine.rs`: Thin effect shell around the pure reducer and atomic durability boundary. |
| [evidence/](evidence/AGENTS.md) | Implementation pieces for `evidence.rs`: Exact-revision gate evidence publication contracts. |
| [product/](product/AGENTS.md) | Exact-target production gate planning and evidence aggregation. |
| [reducer/](reducer/AGENTS.md) | Implementation pieces for `reducer.rs`: Pure deterministic D1 transition, aggregation, and replay. |
| [state/](state/AGENTS.md) | Implementation pieces for `state.rs`: Complete authoritative D1 run state. |
| [wire/](wire/AGENTS.md) | D1-owned typed B3 codecs for reserved families 50, 51, and 52. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
