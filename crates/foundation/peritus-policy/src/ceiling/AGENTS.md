# crates/foundation/peritus-policy/src/ceiling — navigation

## Responsibility

Implementation pieces for `ceiling.rs`: Complete finite authority boundaries, default-deny grants, and immutable denials.

## Files in this folder

| File | Role |
| --- | --- |
| [construction.rs](construction.rs) | Exact ordered validation for protected authority ceilings. |

Owning module: [ceiling.rs](../ceiling.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
