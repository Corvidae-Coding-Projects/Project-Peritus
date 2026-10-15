# crates/app/peritus-daemon/src/terminal/registry — navigation

## Responsibility

Implementation pieces for `registry.rs`: Bounded concurrent registry of live processes and exact terminal attachments.

## Files in this folder

| File | Role |
| --- | --- |
| [launch.rs](launch.rs) | Authorized C2 launch, bounded birth observation, and owned PTY registration. |
| [lookup.rs](lookup.rs) | Exact process and attachment lookup for the live terminal registry. |
| [receipts.rs](receipts.rs) | Bounded exact completion facts, separate from live attachment capacity and ownership. |
| [retirement.rs](retirement.rs) | Reclaim completed disconnected owners before admitting another terminal. |
| [transitions.rs](transitions.rs) | End attachment delivery while retaining bounded exact detach/cancel retry receipts. |

Owning module: [registry.rs](../registry.rs). Inspect that parent before changing the local pieces.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
