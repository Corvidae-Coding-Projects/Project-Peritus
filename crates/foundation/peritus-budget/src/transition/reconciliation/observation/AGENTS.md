# crates/foundation/peritus-budget/src/transition/reconciliation/observation — navigation

## Responsibility

Implementation pieces for `observation.rs`: Monotonic high-water observation transitions.

## Files in this folder

| File | Role |
| --- | --- |
| [in_bounds.rs](in_bounds.rs) | In-ceiling interim and final usage application. |
| [refinement.rs](refinement.rs) | Proof-only bridges from accounting primitives to exact observation steps. |
| [terminal.rs](terminal.rs) | Overrun and exact terminal-observation reconciliation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [in_bounds/](in_bounds/AGENTS.md) | Implementation pieces for `in_bounds.rs`: In-ceiling interim and final usage application. |
| [terminal/](terminal/AGENTS.md) | Implementation pieces for `terminal.rs`: Overrun and exact terminal-observation reconciliation. |

Owning module: [observation.rs](../observation.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
