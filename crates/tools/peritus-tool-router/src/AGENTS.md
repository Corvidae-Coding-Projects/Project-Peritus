# crates/tools/peritus-tool-router/src — navigation

## Responsibility

Exact-authority C4 tool router and sole invocation-permit owner.

## Files in this folder

| File | Role |
| --- | --- |
| [authorization.rs](authorization.rs) | Complete borrowed request and exact committed authority validation. |
| [dispatch.rs](dispatch.rs) | Move-only permit and public dispatch outcomes. |
| [dispatcher.rs](dispatcher.rs) | Object-safe effect and owned active-execution boundaries. |
| [error.rs](error.rs) | Stable router failure vocabulary. |
| [execution.rs](execution.rs) | Router-owned active entry validation and lifecycle helpers. |
| [exposure.rs](exposure.rs) | Canonical role/capability exposure intersection. |
| [intent.rs](intent.rs) | Canonical B3 action-intent payload for one prepared tool call. |
| [lib.rs](lib.rs) | Exact-authority C4 tool router and sole invocation-permit owner. |
| [normalization.rs](normalization.rs) | Truthful bounded normalization of dispatcher failures. |
| [preparation.rs](preparation.rs) | Router-owned effect-free preparation entry point. |
| [recovery.rs](recovery.rs) | Replay and restart recovery classifications. |
| [registry.rs](registry.rs) | Canonical immutable descriptor registry bound to B1 operations. |
| [replay.rs](replay.rs) | Bounded one-use and terminal replay ledger. |
| [router.rs](router.rs) | Stateful bounded dispatch, active ownership, control, deadline, and recovery router. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `ToolAuthorityFacts`, `tool_authority_complete`, `tool_exposure_complete`, `tool_operation_refinement_complete`, `tool_lifecycle_transition_valid`. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
