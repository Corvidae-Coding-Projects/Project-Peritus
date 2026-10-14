# crates/orchestration/peritus-context/src/working — navigation

## Responsibility

Task-local, non-authoritative working state and deterministic reducers.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Provider-independent identity and conversation binding. |
| [delta.rs](delta.rs) | Revision-checked, all-or-nothing working-model updates. |
| [delta_model.rs](delta_model.rs) | Logical exact-result model for ordered working-state deltas. |
| [delta_reducer.rs](delta_reducer.rs) | Deterministic entry upsert and supersession helpers. |
| [entry.rs](entry.rs) | Source-backed investigation records with no evidence or instruction authority. |
| [error.rs](error.rs) | Redaction-safe working-state validation failures. |
| [event.rs](event.rs) | Ordered, replayable inputs to the deterministic working-state reducer. |
| [evidence.rs](evidence.rs) | Stable locators for exact locally archived observations. |
| [invalidation.rs](invalidation.rs) | Exact bounded invalidation reducer. |
| [invalidation_environment.rs](invalidation_environment.rs) | Extensional proof bridge for fixed-point invalidation environments. |
| [invalidation_model.rs](invalidation_model.rs) | Logical model for conservative fixed-point invalidation. |
| [limits.rs](limits.rs) | Explicit local working-state allocation limits. |
| [mod.rs](mod.rs) | Task-local, non-authoritative working state and deterministic reducers. |
| [protocol.rs](protocol.rs) | Host-owned semantic pins; model-authored entries never become policy or effect receipts. |
| [selection.rs](selection.rs) | Working-entry projection through the existing C6 closure selector and typed renderer. |
| [staleness.rs](staleness.rs) | Exact dependency-staleness scan. |
| [state.rs](state.rs) | Immutable observation prefix and bounded investigation model. |
| [state_access.rs](state_access.rs) | Bound reads from immutable working state. |
| [state_model.rs](state_model.rs) | Logical model and public reducer frames for working state. |
| [state_revision.rs](state_revision.rs) | Checked reducer revision advancement. |
| [validation.rs](validation.rs) | Dependency validation and conservative transitive invalidation. |
| [validity.rs](validity.rs) | Explicit workspace dependencies and conservative freshness decisions. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [wire/](wire/AGENTS.md) | Versioned C0-canonical runtime encoding; deterministic semantics remain in the reducers. |

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
