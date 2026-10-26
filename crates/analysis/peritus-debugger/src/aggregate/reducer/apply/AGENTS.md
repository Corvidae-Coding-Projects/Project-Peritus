# crates/analysis/peritus-debugger/src/aggregate/reducer/apply — navigation

## Responsibility

Implementation pieces for `apply.rs`: Exhaustive command-to-event/state reduction.

## Files in this folder

| File | Role |
| --- | --- |
| [model.rs](model.rs) | Optional model-attempt transitions kept separate from deterministic job phases. |

Owning module: [apply.rs](../apply.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
