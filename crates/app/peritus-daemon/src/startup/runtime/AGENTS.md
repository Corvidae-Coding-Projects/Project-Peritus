# crates/app/peritus-daemon/src/startup/runtime — navigation

## Responsibility

Implementation pieces for `runtime.rs`: Live daemon ownership and signal-facing runtime API.

## Files in this folder

| File | Role |
| --- | --- |
| [progress.rs](progress.rs) | Live binding between startup effects, the canonical checkpoint plan, and public lifecycle. |
| [runner.rs](runner.rs) | Ordered startup runner for the live ownership bundle. |
| [teardown.rs](teardown.rs) | Ordered shutdown runner with exact remaining-work accounting. |

Owning module: [runtime.rs](../runtime.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
