# crates/runtime/peritus-process/src/output — navigation

## Responsibility

Implementation pieces for `output.rs`: Bounded stream accounting and retained output.

## Files in this folder

| File | Role |
| --- | --- |
| [spool.rs](spool.rs) | Bounded synchronized local stream spools. |
| [window.rs](window.rs) | Bounded in-memory tail window with stream-preserving reconstruction. |

Owning module: [output.rs](../output.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
