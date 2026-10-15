# crates/tools/peritus-tools-quality/src/execution — navigation

## Responsibility

Implementation pieces for `execution.rs`: Owned quality process projection into the C4 active-execution protocol.

## Files in this folder

| File | Role |
| --- | --- |
| [active.rs](active.rs) | Active quality execution observation, cancellation, and recovery. |
| [failure.rs](failure.rs) | C2 failure normalization for quality execution. |
| [progress.rs](progress.rs) | Ordered C2 quality process progress projection. |
| [terminal.rs](terminal.rs) | Quality terminal, artifact, and candidate B2 evidence projection. |

Owning module: [execution.rs](../execution.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
