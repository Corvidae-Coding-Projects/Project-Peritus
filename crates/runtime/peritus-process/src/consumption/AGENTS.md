# crates/runtime/peritus-process/src/consumption — navigation

## Responsibility

Implementation pieces for `consumption.rs`: Durable one-use authorization consumption and process registry.

## Files in this folder

| File | Role |
| --- | --- |
| [errors.rs](errors.rs) | Maps process-consumption failures into the public typed error surface. |
| [retention.rs](retention.rs) | Counts and retires settled process records within retention policy. |
| [state.rs](state.rs) | Canonical in-memory access to durable process ownership state. |
| [terminal_store.rs](terminal_store.rs) | Durable terminal-result and artifact-publication operations. |
| [watchdog.rs](watchdog.rs) | Platform-specific crash-watchdog configuration. |

Owning module: [consumption.rs](../consumption.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
