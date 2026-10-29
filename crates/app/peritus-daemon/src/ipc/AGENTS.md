# crates/app/peritus-daemon/src/ipc — navigation

## Responsibility

Implementation pieces for `ipc.rs`: Authenticated local IPC and bounded A3 frame transport.

## Files in this folder

| File | Role |
| --- | --- |
| [endpoint.rs](endpoint.rs) | Cross-platform local endpoint facade. |
| [frame.rs](frame.rs) | Allocation-bounded asynchronous PRTS frame transport. |
| [peer.rs](peer.rs) | Canonical authenticated operating-system peer identity. |
| [server.rs](server.rs) | Bounded authenticated local connection acceptor and owned task set, including the shared workspace metadata coordinator. |
| [unix.rs](unix.rs) | Protected Unix-domain endpoint and peer-credential admission. |
| [windows.rs](windows.rs) | Same-user Windows named-pipe endpoint and peer-token admission. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [windows/](windows/AGENTS.md) | Implementation pieces for `windows.rs`: Same-user Windows named-pipe endpoint and peer-token admission. |

Owning module: [ipc.rs](../ipc.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
