# crates/app/peritus-tui/src/runtime/connection — navigation

## Responsibility

Implementation pieces for `connection.rs`: An owned connection attempt is polled beside input, never in front of the UI loop.

## Files in this folder

| File | Role |
| --- | --- |
| [outbox.rs](outbox.rs) | Preserve send order while polling writer backpressure beside terminal input. |

Owning module: [connection.rs](../connection.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
