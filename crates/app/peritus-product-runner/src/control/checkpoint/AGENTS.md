# crates/app/peritus-product-runner/src/control/checkpoint — navigation

## Responsibility

Implementation pieces for `checkpoint.rs`: Bounded checkpoint lineage and restore-journal records without retained file bodies.

## Files in this folder

| File | Role |
| --- | --- |
| [exclusion.rs](exclusion.rs) | Legacy and path-bearing checkpoint exclusion values with stable JSON forms. |
| [restore.rs](restore.rs) | Durable restore outcome and exact transaction bindings. |

Owning module: [checkpoint.rs](../checkpoint.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
