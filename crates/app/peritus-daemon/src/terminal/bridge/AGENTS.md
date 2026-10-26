# crates/app/peritus-daemon/src/terminal/bridge — navigation

## Responsibility

Implementation pieces for `bridge.rs`: One live C2 process control and its bounded A3 attachment projections.

## Files in this folder

| File | Role |
| --- | --- |
| [observation.rs](observation.rs) | Ordered C2 event observation and projection into every live A3 attachment. |
| [owner.rs](owner.rs) | Distinct direct and C4 lifecycle owners; a preview lease never takes the process from C4. |

Owning module: [bridge.rs](../bridge.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
