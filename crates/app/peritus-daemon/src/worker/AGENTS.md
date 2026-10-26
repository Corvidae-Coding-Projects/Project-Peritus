# crates/app/peritus-daemon/src/worker — navigation

## Responsibility

Implementation pieces for `worker.rs`: Structured bounded ownership for scheduler-dispatched Tokio worker tasks.

## Files in this folder

| File | Role |
| --- | --- |
| [cancellation.rs](cancellation.rs) | Cooperative first-request-wins cancellation delivered to one worker task. |
| [error.rs](error.rs) | Stable redaction-safe worker-supervisor failures. |
| [limits.rs](limits.rs) | Positive worker, result, reap, and shutdown bounds. |
| [observation.rs](observation.rs) | Exact scheduler bindings and redaction-safe terminal task observations. |
| [snapshot.rs](snapshot.rs) | Typed lifecycle, count, remaining-work, reap, and shutdown snapshots. |
| [supervisor.rs](supervisor.rs) | Non-cloneable owner of every scheduler-dispatched Tokio worker task. |
| [task.rs](task.rs) | One owned Tokio task, cooperative cancellation sender, and join normalization. |

Owning module: [worker.rs](../worker.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
