# crates/app/peritus-daemon/src/product_run/workbench/checkpoints/capture — navigation

## Responsibility

Implementation pieces for `capture.rs`: Bounded selected-path capture and no-follow folder observations.

## Files in this folder

| File | Role |
| --- | --- |
| [automatic.rs](automatic.rs) | Durable automatic before-images and exact owned-postimage checkpoint settlement. |

Owning module: [capture.rs](../capture.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../../AGENTS.md) · [Repository map](../../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
