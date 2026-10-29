# crates/foundation/peritus-policy/src/scope_selector — navigation

## Responsibility

Implementation pieces for `scope_selector.rs`: Complete parent-relative scope selectors and exact executable refinement.

## Files in this folder

| File | Role |
| --- | --- |
| [containment.rs](containment.rs) | Ordered exact parent-boundary containment for complete selectors. |

Owning module: [scope_selector.rs](../scope_selector.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
