# crates/orchestration/peritus-scheduler/src/durability — navigation

## Responsibility

Implementation pieces for `durability.rs`: Atomic C0 persistence and checked replay loading for one scheduler aggregate.

## Files in this folder

| File | Role |
| --- | --- |
| [binding.rs](binding.rs) | Cross-record transition binding validation before C0 append. |
| [semantics.rs](semantics.rs) | Live creation policy and immutable aggregate schema at the journal owner. |

Owning module: [durability.rs](../durability.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
