# crates/orchestration/peritus-scheduler/src/work — navigation

## Responsibility

Implementation pieces for `work.rs`: Immutable work specifications and authoritative lifecycle records.

## Files in this folder

| File | Role |
| --- | --- |
| [class.rs](class.rs) | Closed execution-class vocabulary used by work and worker selection. |
| [clone_impl.rs](clone_impl.rs) | Semantic clones for admitted and retained work. |
| [phase.rs](phase.rs) | Work recovery and lifecycle phases. |
| [record.rs](record.rs) | Retained work accessors and verified lifecycle mutations. |
| [terminal.rs](terminal.rs) | Truthful terminal work outcomes. |

Owning module: [work.rs](../work.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
