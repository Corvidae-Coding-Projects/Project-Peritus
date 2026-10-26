# crates/analysis/peritus-debugger/src/aggregate/types — navigation

## Responsibility

Implementation pieces for `types.rs`: Bounded values retained by the debugger aggregate.

## Files in this folder

| File | Role |
| --- | --- |
| [failure.rs](failure.rs) | Redaction-safe terminal debugger job failures. |
| [model.rs](model.rs) | Frozen model budgets, retries, work state, and attempt history. |
| [phase.rs](phase.rs) | Closed durable debugger job phases. |
| [records.rs](records.rs) | Selection, deterministic-analysis, report, and publication records. |

Owning module: [types.rs](../types.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
