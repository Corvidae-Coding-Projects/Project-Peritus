# crates/tools/peritus-tools-git/src/dispatcher — navigation

## Responsibility

Implementation pieces for `dispatcher.rs`: Router-owned, cancellable Git execution with joined worker lifetimes.

## Files in this folder

| File | Role |
| --- | --- |
| [context.rs](context.rs) | Complete worker inputs, retaining workspace and artifact-store ownership until join. |
| [execution.rs](execution.rs) | Router owner for the joined Git worker and its cancellation signal. |

Owning module: [dispatcher.rs](../dispatcher.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
