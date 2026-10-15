# crates/app/peritus-product-runner/src/accounting — navigation

## Responsibility

Implementation pieces for `accounting.rs`: Executable invariant/refinement helpers; older proof correspondence is historical. Entry names: `ProductRunProgress`, `model_requests`, `tool_calls`, `retries`, `provider_failovers`.

## Files in this folder

| File | Role |
| --- | --- |
| [limits.rs](limits.rs) | Exact caller-selected elapsed-budget evaluation. |
| [usage.rs](usage.rs) | Exact replacement of the current response's contribution to cumulative usage. |
| [work.rs](work.rs) | Atomic admission of numeric work events before completed-work ceiling checks. |

Owning module: [accounting.rs](../accounting.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
