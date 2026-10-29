# crates/foundation/peritus-budget/src/refinement_model — navigation

## Responsibility

Implementation pieces for `refinement_model.rs`: Concrete immutable-prefix and ancestor-propagation refinement predicates.

## Files in this folder

| File | Role |
| --- | --- |
| [ancestry.rs](ancestry.rs) | Derive propagation along every finite parent path from the checked immediate-parent relation. |

Owning module: [refinement_model.rs](../refinement_model.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
