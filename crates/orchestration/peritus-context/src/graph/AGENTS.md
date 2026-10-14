# crates/orchestration/peritus-context/src/graph — navigation

## Responsibility

Implementation pieces for `graph.rs`: Canonically ordered, bounded context dependency graph.

## Files in this folder

| File | Role |
| --- | --- |
| [cycle.rs](cycle.rs) | Production Kahn traversal connected to the finite cycle model. |
| [cycle_count_init.rs](cycle_count_init.rs) | Edge-driven initialization of exact remaining-dependent counts. |
| [cycle_count_model.rs](cycle_count_model.rs) | Exact remaining-dependent-count model and update lemmas. |
| [cycle_count_update.rs](cycle_count_update.rs) | Edge-driven maintenance of exact remaining-dependent counts. |
| [cycle_counts.rs](cycle_counts.rs) | Exact zero-count selection for the production Kahn traversal. |
| [cycle_edge_model.rs](cycle_edge_model.rs) | Exact edge-occurrence and processed-source count model. |
| [cycle_model.rs](cycle_model.rs) | Finite Kahn elimination model and semantic acyclicity certificate. |
| [cycle_proofs.rs](cycle_proofs.rs) | Completeness lemmas relating semantic DAG certificates to production Kahn traversal. |
| [model.rs](model.rs) | Finite specification of canonical graph identity and dependency admission. |
| [validation.rs](validation.rs) | Production validation and exact lookup for canonical context graphs. |

Owning module: [graph.rs](../graph.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
