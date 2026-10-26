# crates/foundation/peritus-budget/src/reachability/guards — navigation

## Responsibility

Implementation pieces for `guards.rs`: Exact input-state guards for every accepted command outcome.

## Files in this folder

| File | Role |
| --- | --- |
| [lineage_proofs.rs](lineage_proofs.rs) | Constructor lemmas for the finite open-lineage path witness. |
| [operations.rs](operations.rs) | Allocation, begin, and activation admissibility. |
| [outcomes.rs](outcomes.rs) | Observation, finalization, cancellation, and account-lifecycle admissibility. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [outcomes/](outcomes/AGENTS.md) | Implementation pieces for `outcomes.rs`: Observation, finalization, cancellation, and account-lifecycle admissibility. |

Owning module: [guards.rs](../guards.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
