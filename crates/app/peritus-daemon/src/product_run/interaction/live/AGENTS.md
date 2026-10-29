# crates/app/peritus-daemon/src/product_run/interaction/live — navigation

## Responsibility

Implementation pieces for `live.rs`: Live D0 input, provider selection, admission and typed activity adapter.

## Files in this folder

| File | Role |
| --- | --- |
| [modules.rs](modules.rs) | Durable local module-command admission without an inference send or usage charge, including narrowly parsed same-module pause/resume/steer controls during active roles. |
| [request.rs](request.rs) | Durable provider request admission and workbench observation helpers. |
| [review.rs](review.rs) | Read-only review-state access shared by both compilation configurations. |

Owning module: [live.rs](../live.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../../AGENTS.md) · [Repository map](../../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
