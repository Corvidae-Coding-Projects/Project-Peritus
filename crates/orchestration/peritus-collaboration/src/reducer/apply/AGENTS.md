# crates/orchestration/peritus-collaboration/src/reducer/apply — navigation

## Responsibility

Implementation pieces for `apply.rs`: Closed collaboration command application separated from fence and replay orchestration.

## Files in this folder

| File | Role |
| --- | --- |
| [terminal.rs](terminal.rs) | Task completion, cancellation, pause, and aggregate terminal truth. |

Owning module: [apply.rs](../apply.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
