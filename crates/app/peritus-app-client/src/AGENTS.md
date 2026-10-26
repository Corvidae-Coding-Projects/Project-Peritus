# crates/app/peritus-app-client/src — navigation

## Responsibility

Shared local transport contracts for Peritus user interfaces.

## Files in this folder

| File | Role |
| --- | --- |
| [connection.rs](connection.rs) | Negotiated local daemon connections with explicitly serialized requests. |
| [error.rs](error.rs) | Interface-independent transport failures. |
| [events.rs](events.rs) | Event and control exchanges for dedicated subscription connections. |
| [frame.rs](frame.rs) | Bounded PRTS framing over the platform's local IPC stream. |
| [identity.rs](identity.rs) | Explicit identities retained across request settlement and reconnect. |
| [lib.rs](lib.rs) | Shared local transport contracts for Peritus user interfaces. |
| [request.rs](request.rs) | Serialized request exchanges with exact response binding. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
