# crates/app/peritus-daemon/src/domain/dispatch — navigation

## Responsibility

Implementation pieces for `dispatch.rs`: Exact family routing and single-writer domain commits.

## Files in this folder

| File | Role |
| --- | --- |
| [debugger.rs](debugger.rs) | Family-82 debugger aggregate dispatch. |
| [evaluation.rs](evaluation.rs) | Family-85 evaluation aggregate dispatch. |
| [evolution.rs](evolution.rs) | Family-88 campaign and family-91 production-pointer dispatch. |
| [harness.rs](harness.rs) | Family-79 harness aggregate dispatch. |

Owning module: [dispatch.rs](../dispatch.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
