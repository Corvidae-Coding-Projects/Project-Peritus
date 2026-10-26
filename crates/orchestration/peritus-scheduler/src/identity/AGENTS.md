# crates/orchestration/peritus-scheduler/src/identity — navigation

## Responsibility

Implementation pieces for `identity.rs`: Scheduler-owned stable nonzero identities.

## Files in this folder

| File | Role |
| --- | --- |
| [order.rs](order.rs) | Exact identity equality and strict canonical byte-order laws. |

Owning module: [identity.rs](../identity.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
