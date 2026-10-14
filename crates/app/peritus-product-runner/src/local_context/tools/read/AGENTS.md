# crates/app/peritus-product-runner/src/local_context/tools/read — navigation

## Responsibility

Implementation pieces for `read.rs`: Exact byte-range retrieval and bounded literal-search/state pages with scope-bound cursors.

## Files in this folder

| File | Role |
| --- | --- |
| [cursor.rs](cursor.rs) | Continuation cursor creation/validation for exact working-entry and search pages. |
| [state.rs](state.rs) | Bounded working-state entry pages and partial-entry projections. |

Owning module: [read.rs](../read.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
