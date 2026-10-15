# crates/orchestration/peritus-scheduler/src/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Complete authoritative scheduler state.

## Files in this folder

| File | Role |
| --- | --- |
| [clone_impl.rs](clone_impl.rs) | Semantic clone for the complete scheduler aggregate. |
| [lookup.rs](lookup.rs) | Exact lookups in canonical state using the production binary-search algorithm. |
| [mutation.rs](mutation.rs) | Reducer-only state mutation and derived invariant maintenance. |
| [ordering.rs](ordering.rs) | Canonical identity ordering required by production binary searches and insertion. |
| [queue.rs](queue.rs) | Exact queue accounting for each persisted scheduler semantics. |
| [terminal.rs](terminal.rs) | Truthful scheduler terminal evaluation. |
| [validation.rs](validation.rs) | Validation of decoded inert scheduler checkpoints. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [mutation/](mutation/AGENTS.md) | Implementation pieces for `mutation.rs`: Reducer-only state mutation and derived invariant maintenance. |
| [terminal/](terminal/AGENTS.md) | Implementation pieces for `terminal.rs`: Truthful scheduler terminal evaluation. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
