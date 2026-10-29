# crates/state/peritus-leases/src/state — navigation

## Responsibility

Implementation pieces for `state.rs`: Privately represented aggregate state and checked immutable projections.

## Files in this folder

| File | Role |
| --- | --- |
| [validity.rs](validity.rs) | Exact aggregate-validity predicates and their executable refinements. |

Owning module: [state.rs](../state.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
