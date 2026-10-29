# crates/orchestration/peritus-orchestrator/src/binding — navigation

## Responsibility

Implementation pieces for `binding.rs`: Immutable contract, run, child-domain, revision, and limit binding.

## Files in this folder

| File | Role |
| --- | --- |
| [quality_cycle.rs](quality_cycle.rs) | Fresh per-candidate D1 and D3 aggregate identity binding. |

Owning module: [binding.rs](../binding.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
