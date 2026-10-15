# crates/orchestration/peritus-memory/src/retrieval — navigation

## Responsibility

Implementation pieces for `retrieval.rs`: Deterministic filter, rank, and budget retrieval planning.

## Files in this folder

| File | Role |
| --- | --- |
| [filter.rs](filter.rs) | Ordered fail-closed retrieval filters. |
| [output.rs](output.rs) | Immutable ranked candidates, exclusions, and complete retrieval plans. |
| [plan.rs](plan.rs) | Transactional retrieval orchestration and complete explanations. |
| [ranking.rs](ranking.rs) | Bounded integer retrieval ranking with stable identity tie-breaking. |
| [types.rs](types.rs) | Checked retrieval inputs and immutable explainable outputs. |

Owning module: [retrieval.rs](../retrieval.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
