# crates/app/peritus-daemon/src/session — navigation

## Responsibility

Implementation pieces for `session.rs`: Authenticated A3 session establishment and connection ownership.

## Files in this folder

| File | Role |
| --- | --- |
| [connection.rs](connection.rs) | One bounded A3 connection owner with independent read-only provider discovery. |
| [heartbeat.rs](heartbeat.rs) | Per-connection monotonic A3 heartbeat exchange. |
| [negotiation.rs](negotiation.rs) | Durable peer-bound A3 negotiation. |
| [request.rs](request.rs) | Closed A3 request dispatch for one authenticated connection. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [connection/](connection/AGENTS.md) | Implementation pieces for `connection.rs`: One bounded A3 connection owner with independent read-only provider discovery. |
| [request/](request/AGENTS.md) | Implementation pieces for `request.rs`: Closed A3 request dispatch for one authenticated connection. |

Owning module: [session.rs](../session.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
