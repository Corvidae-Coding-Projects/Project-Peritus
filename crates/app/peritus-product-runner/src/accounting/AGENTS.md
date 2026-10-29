# crates/app/peritus-product-runner/src/accounting — navigation

## Responsibility

Overflow-safe production accounting for `accounting.rs`. These helpers apply atomic work events, replace response-scoped provider usage without double counting, and evaluate caller-selected elapsed ceilings against completed observations.

## Files in this folder

| File | Role |
| --- | --- |
| [limits.rs](limits.rs) | Exact caller-selected elapsed-budget evaluation. |
| [usage.rs](usage.rs) | Exact replacement of the current response's contribution to cumulative usage. |
| [work.rs](work.rs) | Atomic admission of numeric work events before completed-work ceiling checks. |

Owning module: [accounting.rs](../accounting.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
