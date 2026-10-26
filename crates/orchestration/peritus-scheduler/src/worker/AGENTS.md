# crates/orchestration/peritus-scheduler/src/worker — navigation

## Responsibility

Implementation pieces for `worker.rs`: Worker descriptors, lifecycle, and durable reservations.

## Files in this folder

| File | Role |
| --- | --- |
| [admission.rs](admission.rs) | Exact checked admission for immutable worker descriptors. |
| [clone_impl.rs](clone_impl.rs) | Semantic clones for workers and durable reservations. |

Owning module: [worker.rs](../worker.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
