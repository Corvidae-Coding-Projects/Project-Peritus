# crates/tools/peritus-tools-fs/src/dispatcher — navigation

## Responsibility

Implementation pieces for `dispatcher.rs`: Router-authorized filesystem dispatcher adapters.

## Files in this folder

| File | Role |
| --- | --- |
| [execution.rs](execution.rs) | Filesystem dispatcher execution routing and lifecycle. |
| [mutation_execution.rs](mutation_execution.rs) | Owned filesystem mutation workers, cancellation, completion, and restart recovery. |
| [operations.rs](operations.rs) | Compiles/executes filesystem operations with content and caller binding. |
| [read_execution.rs](read_execution.rs) | Owned cancellable read workers and completion/failure observations. |

Owning module: [dispatcher.rs](../dispatcher.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
