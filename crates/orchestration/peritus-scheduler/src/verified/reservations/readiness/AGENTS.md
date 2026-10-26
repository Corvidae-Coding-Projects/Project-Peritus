# crates/orchestration/peritus-scheduler/src/verified/reservations/readiness — navigation

## Responsibility

Implementation pieces for `readiness.rs`: Reducer admission facts connecting lifecycle and durable dispatch history.

## Files in this folder

| File | Role |
| --- | --- |
| [clone.rs](clone.rs) | Lifecycle compatibility transport across exact semantic sequence clones. |
| [insertion.rs](insertion.rs) | Lifecycle compatibility across inactive work admission. |
| [phase.rs](phase.rs) | Bidirectional lifecycle compatibility between work and live reservations. |
| [release.rs](release.rs) | Lifecycle correspondence after exact reservation release and work update. |
| [start.rs](start.rs) | Lifecycle correspondence after start acknowledgement. |
| [update.rs](update.rs) | Readiness preservation for compatible standalone work phase updates. |

Owning module: [readiness.rs](../readiness.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
