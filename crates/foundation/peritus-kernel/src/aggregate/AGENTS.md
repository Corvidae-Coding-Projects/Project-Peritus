# crates/foundation/peritus-kernel/src/aggregate — navigation

## Responsibility

Implementation pieces for `aggregate.rs`: Complete lifecycle aggregate and genesis transition.

## Files in this folder

| File | Role |
| --- | --- |
| [lookup.rs](lookup.rs) | Bounded aggregate lookup and liveness queries. |
| [validation.rs](validation.rs) | Executable aggregate invariant checks. |

Owning module: [aggregate.rs](../aggregate.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
