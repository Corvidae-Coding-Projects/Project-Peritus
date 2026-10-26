# crates/app/peritus-daemon/src/terminal — navigation

## Responsibility

Implementation pieces for `terminal.rs`: Live C2 process control projected through bounded A3 terminal attachments.

## Files in this folder

| File | Role |
| --- | --- |
| [attachment.rs](attachment.rs) | One attachment's A3 state, bounded pending delivery, and output fencing. |
| [bridge.rs](bridge.rs) | One live C2 process control and its bounded A3 attachment projections. |
| [error.rs](error.rs) | Stable terminal-bridge failures with preserved protocol and process sources. |
| [limits.rs](limits.rs) | Fixed memory, process, attachment, replay, delivery, and startup bounds. |
| [qualification.rs](qualification.rs) | Bounded operational qualification of the host's real combined PTY stream. |
| [recovery.rs](recovery.rs) | Honest restart classification when no in-memory PTY control can survive daemon loss. |
| [registry.rs](registry.rs) | Bounded concurrent registry of live processes and exact terminal attachments. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bridge/](bridge/AGENTS.md) | Implementation pieces for `bridge.rs`: One live C2 process control and its bounded A3 attachment projections. |
| [qualification/](qualification/AGENTS.md) | Implementation pieces for `qualification.rs`: Bounded operational qualification of the host's real combined PTY stream. |
| [registry/](registry/AGENTS.md) | Implementation pieces for `registry.rs`: Bounded concurrent registry of live processes and exact terminal attachments. |

Owning module: [terminal.rs](../terminal.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
