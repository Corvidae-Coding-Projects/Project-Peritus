# crates/foundation/peritus-budget/src/reachability/guards/outcomes — navigation

## Responsibility

Implementation pieces for `outcomes.rs`: Observation, finalization, cancellation, and account-lifecycle admissibility.

## Files in this folder

| File | Role |
| --- | --- |
| [lifecycle.rs](lifecycle.rs) | Runtime bridge for exact seal and close outcome guards. |

Owning module: [outcomes.rs](../outcomes.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
