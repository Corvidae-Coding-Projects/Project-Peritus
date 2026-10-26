# crates/app/peritus-product-runner/src/workspace_delivery — navigation

## Responsibility

Implementation pieces for `workspace_delivery.rs`: Delivery-specific adapters around the single production execution pipeline.

## Files in this folder

| File | Role |
| --- | --- |
| [scope_store.rs](scope_store.rs) | Transactional SQLite path index and original before-image migration; bounded reader snapshots invalidated by database identity, SQLite data version and legacy binding. |
| [scope.rs](scope.rs) | In-place comparison evidence and captured candidate axes; exact-file automatic checkpoints, directory-descendant tracking, and binary-aware display of durable previews. No Git. |

Owning module: [workspace_delivery.rs](../workspace_delivery.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
