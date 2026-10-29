# crates/app/peritus-daemon/src/product_run/workbench/checkpoints/rewind — navigation

## Responsibility

Implementation pieces for `rewind.rs`: Reviewed rewind preview, exact C1 application, and truthful durable settlement.

## Files in this folder

| File | Role |
| --- | --- |
| [plan.rs](plan.rs) | Exact filesystem patch planning for a confirmed rewind preview. |
| [preview.rs](preview.rs) | Read-only rewind preview over exact stored checkpoint facts. |

Owning module: [rewind.rs](../rewind.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
