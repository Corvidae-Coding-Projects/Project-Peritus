# crates/orchestration/peritus-scheduler/src/state/mutation/dependency_round — navigation

## Responsibility

Implementation pieces for `dependency_round.rs`: Exact ordered collection of dependency refresh changes.

## Files in this folder

| File | Role |
| --- | --- |
| [apply.rs](apply.rs) | Finite application of one immutable dependency-refresh change set. |

Owning module: [dependency_round.rs](../dependency_round.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
