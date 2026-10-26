# crates/tools/peritus-tools-shell/src/execution — navigation

## Responsibility

Implementation pieces for `execution.rs`: Owned C2 process to C4 active-execution adapter.

## Files in this folder

| File | Role |
| --- | --- |
| [active.rs](active.rs) | Active owned-process control, observation, finalization, and recovery. |
| [failure.rs](failure.rs) | Stable lower-boundary failure normalization. |
| [progress.rs](progress.rs) | Ordered C2 event projection into C4 progress. |
| [terminal.rs](terminal.rs) | Truthful C2 terminal and artifact projection. |

Owning module: [execution.rs](../execution.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
